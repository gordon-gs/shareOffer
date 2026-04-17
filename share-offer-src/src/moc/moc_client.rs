use clap::{Parser, command};
use crossbeam_channel::unbounded;
use fproto::stream_frame::CommonExchangeOrderFields;
use fproto::stream_frame::tdgw_bin::heartbeat::Heartbeat;
use fproto::stream_frame::tdgw_bin::logon::Logon;
use fproto::stream_frame::tdgw_bin::logout::Logout;
use fproto::stream_frame::tdgw_bin::new_order_single::NewOrderSingle;
use rustyline::DefaultEditor;
use rustyline::ExternalPrinter;
use share_offer::route;
use std::io::ErrorKind;
use std::io::{self, BufRead, Read, Write};
use std::net::TcpStream;
use std::os::unix::prelude;
use std::slice::RSplit;
use std::sync::mpsc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::task::JoinError;

/// TCP 服务器配置
#[derive(Parser, Debug)]
#[command(name = "moc_client")]
#[command(about = "共享报盘moc客户端", long_about = None)]
struct Args {
    /// 服务器地址
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// 监听端口
    #[arg(short = 'P', long, default_value_t = 18001)]
    port: u16,

    ///oms_id
    #[arg(short = 'O', long, default_value_t = 48)]
    oms_id: u16,

    ///share_offer_id
    #[arg(short = 'S', long, default_value_t = 49)]
    share_offer_id: u16,

    ///target_gw_id
    #[arg(short = 'T', long, default_value_t = 49)]
    target_gw_id: u16,

    ///contractnum
    #[arg(short = 'C', long, default_value_t = 111)]
    contractnum: i64,

    /// 是否自动登录并启动心跳 [default: false]
    #[arg(short = 'A', long, default_value_t = false)]
    auto_logon: bool,
}

fn main() {
    let args = Args::parse();
    let addr = Arc::new(format!("{}:{}", args.host, args.port));

    println!("====================================");
    println!(
        "  客户端启动, oms_id:{} , share_offer_id:{}, target_gw_id:{}, start contractnum:{}",
        args.oms_id, args.share_offer_id, args.target_gw_id, args.contractnum
    );
    println!("====================================");
    println!("正在连接服务器 {}...", addr);

    let stream = match TcpStream::connect(addr.as_ref()) {
        Ok(s) => {
            println!("✓ 已连接到服务器！\n");
            s
        }
        Err(e) => {
            eprintln!("✗ 连接失败: {}", e);
            eprintln!("提示: 请确保服务器已启动");
            return;
        }
    };

    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    // 克隆 stream，一个用于读，一个用于写
    let mut write_stream = stream.try_clone().expect("无法克隆 stream");
    let mut read_stream = stream;

    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    // 用于控制读线程的退出
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let mut begin_heatbeat = Arc::new(AtomicBool::new(false));
    let mut begin_heatbeat_clone = begin_heatbeat.clone();
    let mut rl = DefaultEditor::new().unwrap();
    let mut printer = rl.create_external_printer().expect("can't create printer");
    let mut cur_contractnum = args.contractnum;

    // 如果启用了自动登录，则发送 logon 并启动心跳
    if args.auto_logon {
        let mut logon = Logon::new();
        logon.set_heart_bt_int(3);
        logon.set_prtcl_version_from_string("1.11");
        logon.set_sender_comp_id_from_string("app-01");
        logon.set_target_comp_id_from_string("SO_01");
        let real_byte = logon.as_bytes_big_endian();
        println!("自动发送 logon: {}", logon);
        if let Err(e) = tx.send(real_byte) {
            eprintln!("✗ 自动发送 logon 失败: {}", e);
            return;
        }
        begin_heatbeat.store(true, Ordering::Relaxed);
        println!("自动启动心跳功能");
    }

    let send_thread = thread::spawn(move || {
        loop {
            // 设置 2 秒超时
            match rx.recv_timeout(Duration::from_secs(9)) {
                Ok(msg) => {
                    let bytes = msg.as_slice();
                    printer
                        .print(format!("send success !\n"))
                        .expect("打印失败");
                    // 显示十六进制形式
                    printer.print(format!("十六进制: \n")).expect("打印失败");
                    for byte in bytes {
                        print!("{:02X} ", byte);
                    }
                    println!();

                    // 显示二进制形式
                    printer.print(format!("二进制: \n")).expect("打印失败");
                    for byte in bytes {
                        print!("{:08b} ", byte);
                    }
                    println!();
                    println!("{}", "-".repeat(60));
                    if let Err(e) = write_stream.write_all(bytes) {
                        eprintln!("发送失败,发送线程退出: {}", e);
                        break;
                    } else {
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // 发送一个心跳包给服务器，或者只是打印一个日志
                    printer
                        .print(format!(
                            "heartbeat status: {}",
                            begin_heatbeat_clone.load(Ordering::Relaxed)
                        ))
                        .expect("打印失败");
                    if begin_heatbeat_clone.load(Ordering::Relaxed) {
                        let mut heartbeat = Heartbeat::new();
                        heartbeat.filled_head_and_tail();
                        let real_byte = heartbeat.as_bytes_big_endian();
                        let bytes = real_byte.as_slice();
                        if let Err(e) = write_stream.write_all(bytes) {
                            eprintln!("heatbeat 发送失败，发送线程退出: {}", e);
                            break;
                        }
                        printer
                            .print(format!("send heartbeat: {}", heartbeat))
                            .expect("打印失败");
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // 主线程的 tx 被丢弃了，channel 已关闭
                    println!("[发送线程] Channel 已断开，退出线程");
                    break;
                }
            }
        }
    });

    // 启动读取线程
    let read_thread = thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        while running_clone.load(Ordering::Relaxed) {
            match read_stream.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    let data = &buffer[..n];

                    // 显示字符串形式
                    println!("\n[收到服务器数据] 来自:{}", addr.as_str());

                    // 显示字符串形式
                    println!("字符串: {}", String::from_utf8_lossy(data));

                    // 显示十六进制形式
                    print!("十六进制: ");
                    for byte in data {
                        print!("{:02X} ", byte);
                    }
                    println!();

                    // 显示二进制形式
                    print!("二进制: ");
                    for byte in data {
                        print!("{:08b} ", byte);
                    }
                    println!();
                    println!("{}", "-".repeat(60));

                    print!("> ");
                    io::stdout().flush().unwrap();
                }
                Ok(_) => {
                    println!("\n服务器已断开连接");
                    running_clone.store(false, Ordering::Relaxed);
                    break;
                }
                Err(e) => {
                    if running_clone.load(Ordering::Relaxed) {
                        match e.kind() {
                            ErrorKind::WouldBlock | ErrorKind::TimedOut => {
                                //pass
                            }
                            _ => {
                                eprintln!("\n网络错误: {},退出接收线程", e);
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    });

    println!("====================================");
    println!("  请输入要发送的消息");
    println!("  支持输入logon/heartbeat/order，其余则会直接发送字符串数组");
    println!("  输入 'quit' 或 'exit' 退出");
    println!("====================================\n");

    loop {
        let readline = rl.readline(">");

        if !running.load(Ordering::Relaxed) {
            break;
        }

        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if trimmed == "quit" || trimmed == "exit" {
                    println!("退出客户端...");
                    running.store(false, Ordering::Relaxed);
                    drop(tx);
                    break;
                }
                let mut real_byte: Vec<u8>;

                match trimmed {
                    "order" => {  
                        let mut new_order = NewOrderSingle::new();
                            new_order.set_contract_number_from_i64(cur_contractnum);
                            new_order.set_order_qty_from_i32(2);
                            let mut route_info = route::RouteInfo::default();
                            route_info.gw_id = args.target_gw_id;
                            route_info.oms_id = args.oms_id;
                            route_info.share_offer_id = args.share_offer_id;
                            new_order.set_user_info(route_info.get_tdgw_user_info());
                            new_order.filled_head_and_tail();
                            real_byte = new_order.as_bytes_big_endian();
                            println!("send new Order: {}", new_order);
                            // 发送数据
                            if let Err(e) = tx.send(real_byte.clone()) {
                                eprintln!("✗ 发送失败: {}", e);
                                running.store(false, Ordering::Relaxed);
                                break;
                            }
                            cur_contractnum += 1;
                        }
                    "order_no_gw" => {
                        let mut new_order = NewOrderSingle::new();
                        new_order.set_contract_number_from_i64(cur_contractnum);
                        new_order.set_order_qty_from_i32(3);
                        let mut route_info = route::RouteInfo::default();
                        route_info.gw_id = 48;
                        route_info.oms_id = args.oms_id;
                        route_info.share_offer_id = args.share_offer_id;
                        new_order.set_user_info(route_info.get_tdgw_user_info());
                        new_order.filled_head_and_tail();
                        real_byte = new_order.as_bytes_big_endian();
                        println!("send order_no_gw: {}", new_order);
                        // 发送数据
                        if let Err(e) = tx.send(real_byte.clone()) {
                            eprintln!("✗ 发送失败: {}", e);
                            running.store(false, Ordering::Relaxed);
                            break;
                        }
                        cur_contractnum += 1;
                    }
                    "order_userinfo_is_null" => {
                        let mut new_order = NewOrderSingle::new();
                        new_order.set_contract_number_from_i64(cur_contractnum);
                        new_order.set_order_qty_from_i32(4);
                        let mut route_info = route::RouteInfo::default();
                        route_info.gw_id = 32;
                        route_info.oms_id = 32;
                        route_info.share_offer_id = 32;
                        new_order.set_user_info(route_info.get_tdgw_user_info());
                        new_order.filled_head_and_tail();
                        real_byte = new_order.as_bytes_big_endian();
                        println!("send order_userinfo_is_null: {}", new_order);
                        // 发送数据
                        if let Err(e) = tx.send(real_byte.clone()) {
                            eprintln!("✗ 发送失败: {}", e);
                            running.store(false, Ordering::Relaxed);
                            break;
                        }
                        cur_contractnum += 1;
                    }
                    "logon" => {
                        let mut logon = Logon::new();
                        logon.set_heart_bt_int(3);
                        logon.set_prtcl_version_from_string("1.11");
                        logon.set_sender_comp_id_from_string("app-01");
                        logon.set_target_comp_id_from_string("SO_01");
                        real_byte = logon.as_bytes_big_endian();
                        println!("send logon: {}", logon);
                        if let Err(e) = tx.send(real_byte.clone()) {
                            eprintln!("✗ 发送失败: {}", e);
                            running.store(false, Ordering::Relaxed);
                            break;
                        }
                    }

                    "logout" => {
                        let mut logout = Logout::new();
                        logout.set_text_from_string("moc client logout");
                        logout.filled_head_and_tail();
                        real_byte = logout.as_bytes_big_endian();
                        println!("send logout: {}", logout);
                        if let Err(e) = tx.send(real_byte.clone()) {
                            eprintln!("✗ logout 发送失败: {}", e);
                            running.store(false, Ordering::Relaxed);
                            break;
                        }
                    }

                    "heartbeat" => {
                        begin_heatbeat.store(true, Ordering::Relaxed);
                        println!("begin send heartbeat:");
                    }
                    _ => {
                        println!(" unsupport command !")
                    }
                }
            }
            Err(e) => {
                eprintln!("读取输入失败: {}", e);
                running.store(false, Ordering::Relaxed);
                break;
            }
        }
    }

    // 等待读线程结束
    let _ = read_thread.join();
    let _ = send_thread.join();

    println!("\n客户端已关闭");
}
