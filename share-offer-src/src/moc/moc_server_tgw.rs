use clap::Parser;
use crossbeam_channel::unbounded;
use fproto::stream_frame::tgw_bin::{
    execution_report_200115::ExecutionReport200115, execution_report_204115::ExecutionReport204115,
    execution_report_response_200102::ExecutionReportResponse200102,
    execution_report_response_204102::ExecutionReportResponse204102, heartbeat::Heartbeat,
    logon::Logon, logout::Logout, new_order_100101::NewOrder100101,
    new_order_104101::NewOrder104101, platform_state_info::PlatformStateInfo,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};
use std::{string, thread};

use lazy_static::lazy_static;

lazy_static! {
    //心跳
    pub static ref TDGW_HEARTHB_MSG:Heartbeat={
        let heart_beat = Heartbeat::default();
        heart_beat
    };
}

/// TCP 服务器配置
#[derive(Parser, Debug)]
#[command(name = "moc_server_tgw")]
#[command(about = "共享报盘moc_tgw服务器", long_about = None)]
struct Args {
    /// 服务器地址
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// 监听端口
    #[arg(short, long, default_value_t = 18002)]
    port: u16,

    /// 平台id
    #[arg(long, default_value_t = 0)]
    platform: u16,
}

fn genertae_platform_state_msg(platform_id: u16) -> [PlatformStateInfo; 5] {
    let mut not_open_state = PlatformStateInfo::default();
    not_open_state.set_platform_id(platform_id);
    not_open_state.set_platform_state(0);
    not_open_state.filled_head_and_tail();

    let mut open_up_coming = PlatformStateInfo::default();
    open_up_coming.set_platform_id(platform_id);
    open_up_coming.set_platform_state(1);
    open_up_coming.filled_head_and_tail();

    let mut open_state = PlatformStateInfo::default();
    open_state.set_platform_id(platform_id);
    open_state.set_platform_state(2);
    open_state.filled_head_and_tail();

    let mut halt_state = PlatformStateInfo::default();
    halt_state.set_platform_id(platform_id);
    halt_state.set_platform_state(3);
    halt_state.filled_head_and_tail();

    let mut close_state = PlatformStateInfo::default();
    close_state.set_platform_id(platform_id);
    close_state.set_platform_state(4);
    close_state.filled_head_and_tail();

    [
        not_open_state,
        open_up_coming,
        open_state,
        halt_state,
        close_state,
    ]
}

fn handle_client(mut stream: TcpStream, platform_id: u16) {
    let peer_addr = stream.peer_addr().unwrap();
    println!("\n[新连接] 客户端地址: {}", peer_addr);
    println!("{}", "=".repeat(60));

    let mut buffer = [0u8; 1024];
    let mut msgType: u32 = 0;

    let mut heart_bt_int: i32 = 0;
    let mut last_write_time = Instant::now();
    let mut heartbeat_timeout = Duration::from_secs(30);

    let mut is_logon: bool = false;
    //let mut send_cnt: u32 = 0;

    let plateform_state_msgs = genertae_platform_state_msg(platform_id);

    // 给TCP流设读超时（500毫秒），避免read一直阻塞
    if let Err(e) = stream.set_read_timeout(Some(Duration::from_millis(500))) {
        eprintln!("设置读超时失败: {}", e);
        return;
    }

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                println!("\n[断开连接] 客户端 {} 已断开", peer_addr);
                break;
            }
            Ok(n) => {
                let data = &buffer[..n];

                // 显示字符串形式
                println!("\n[收到数据] 来自 {},长度:{}", peer_addr, n);
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
                //println!("send_cnt: {}", send_cnt);

                // 解析 msgType (第0-3字节)
                if n >= 8 {
                    msgType = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
                    println!("msgType: {}", msgType);
                }

                // 回显逻辑
                match msgType {
                    1 => {
                        //解析心跳间隔
                        if msgType == 1 && n >= 82 {
                            heart_bt_int = i32::from_be_bytes([
                                buffer[48], buffer[49], buffer[50], buffer[51],
                            ]);
                            heartbeat_timeout = Duration::from_secs(heart_bt_int as u64); // 心跳超时时间
                            println!("heart_bt_int: {}", heart_bt_int);
                        }
                        // 回复登录
                        if let Err(e) = stream.write_all(&data) {
                            eprintln!("发送登录响应失败: {}", e);
                            break;
                        } else {
                            is_logon = true;
                            println!("回复logon成功");
                        }

                        //回复平台状态
                        if let Err(e) = stream
                            .write_all(&plateform_state_msgs[2].as_bytes_big_endian().as_slice())
                        {
                            eprintln!("发送平台open失败: {}", e);
                            break;
                        } else {
                            println!("发送平台open成功");
                        }

                        //回复平台信息
                    }
                    100101 => {
                        let mut new_order = NewOrder100101::new();
                        new_order.copy_from_big_endian(data);
                        println!("recvie new Order :{:?}", new_order);
                        let mut resp = ExecutionReportResponse200102::default();
                        resp.set_reporting_pbuid_from_string("077100");
                        resp.set_partition_no(4);
                        resp.set_report_index(1);
                        resp.set_exec_type(56);
                        resp.set_ord_rej_reason(4012);
                        resp.set_user_info(new_order.get_user_info().clone());
                        // 回复ExecutionReportResponse 失败
                        if let Err(e) = stream.write_all(&resp.as_bytes_big_endian().as_slice()) {
                            eprintln!("发送确认响应失败: {}", e);
                            break;
                        } else {
                            is_logon = true;
                            println!("发送拒单响应：{:?}", resp)
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                // 处理超时错误（WouldBlock是正常超时，继续循环；其他错误退出）
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    continue; // 超时，不退出，继续执行心跳检查
                } else {
                    eprintln!("读取数据失败: {}", e);
                    break;
                }
            }
        }

        println!(
            "心跳检查: is_logon={}, 距上次发送耗时={:?}, 心跳间隔={:?}",
            is_logon,
            last_write_time.elapsed().as_secs_f64(),
            heartbeat_timeout.as_secs_f64()
        );

        if is_logon && last_write_time.elapsed() > heartbeat_timeout {
            match stream.write_all(TDGW_HEARTHB_MSG.as_bytes_big_endian().as_slice()) {
                Ok(_) => {
                    last_write_time = Instant::now();
                }
                Err(e) => {
                    eprintln!("发送心跳失败: {}", e);
                    break;
                }
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    let addr = format!("{}:{}", args.host, args.port);

    let platform_id = args.platform;

    let listener = TcpListener::bind(addr.clone()).expect("无法绑定地址");

    println!("====================================");
    println!("  Tgw服务器已启动");
    println!("  监听地址: {},平台ID：{}", addr, platform_id);
    println!("====================================");
    println!("\n等待客户端连接...\n");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    handle_client(stream, platform_id);
                });
            }
            Err(e) => {
                eprintln!("连接失败: {}", e);
            }
        }
    }
}
