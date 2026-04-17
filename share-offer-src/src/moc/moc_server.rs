use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{string, thread};

use clap::Parser;
use fproto::stream_frame::tdgw_bin::exec_rpt_info::{ExecRptInfo, NoGroups4, NoGroups5};
use fproto::stream_frame::tdgw_bin::exec_rpt_sync_rsp::{ExecRptSyncRsp, NoGroups3};
use fproto::stream_frame::tdgw_bin::execution_report_response::ExecutionReportResponse;
use fproto::stream_frame::tdgw_bin::heartbeat::Heartbeat;
use fproto::stream_frame::tdgw_bin::logon::Logon;
use fproto::stream_frame::tdgw_bin::logout::Logout;
use fproto::stream_frame::tdgw_bin::new_order_single::NewOrderSingle;
use fproto::stream_frame::tdgw_bin::platform_state::PlatformState;
use lazy_static::lazy_static;
use tracing_subscriber::layer;
lazy_static! {

//心跳
pub static ref TDGW_HEARTHB_MSG:Heartbeat={
    let heart_beat = Heartbeat::default();
    heart_beat
};

//ExecRptInfo
//pub static ref EXEC_RPT_INFO:[u8;70]=[0, 0, 0, 208, 0, 0, 0, 0, 0, 0, 0, 76, 0, 0, 0, 50, 0, 0, 0, 1, 51, 55, 49, 54, 55, 32, 32, 32, 0, 9, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 20, 0, 0, 3, 223, 0, 0, 3, 224, 0, 0, 0, 174];
pub static ref EXEC_RPT_INFO:ExecRptInfo={
    let mut exec_rpt_info = ExecRptInfo::default();
    exec_rpt_info.set_msg_seq_num(77);
    exec_rpt_info.set_msg_body_len(98);
    let mut no_group_4 = NoGroups4::default();
    no_group_4.set_pbu_from_string("37167");
    let mut v_no_group_4 = vec![];
    v_no_group_4.push(no_group_4);
    exec_rpt_info.set_no_groups_4(&v_no_group_4);
    let mut v_no_group_5 = vec![];
    for i in 1..6{
      let mut no_group_5 = NoGroups5::default();
      no_group_5.set_set_id(i as u32);
      v_no_group_5.push(no_group_5);
    }
    let mut no_group_5_20 = NoGroups5::default();
      no_group_5_20.set_set_id(20);
      v_no_group_5.push(no_group_5_20);
      let mut no_group_5_991 = NoGroups5::default();
      no_group_5_991.set_set_id(991);
      v_no_group_5.push(no_group_5_991);
      let mut no_group_5_992 = NoGroups5::default();
      no_group_5_992.set_set_id(992);
      v_no_group_5.push(no_group_5_992);
    exec_rpt_info.set_no_groups_5(&v_no_group_5);
    exec_rpt_info.set_checksum(226);
    exec_rpt_info.filled_head_and_tail();
    exec_rpt_info

};

//ExecRptSyncRsp
//pub static ref EXEC_RPT_SYNC_RSP:[u8;118] =[0, 0, 0, 207, 0, 0, 0, 0, 0, 0, 0, 77, 0, 0, 0, 98, 0, 1, 51, 55, 49, 54, 55, 32, 32, 32, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 83, 117, 99, 99, 101, 115, 115, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 0, 0, 0, 226];

pub static ref EXEC_RPT_SYNC_RSP:ExecRptSyncRsp = {
    let mut exec_rpt_sync_rsp = ExecRptSyncRsp::default();
    exec_rpt_sync_rsp.set_msg_seq_num(76);
    exec_rpt_sync_rsp.set_msg_body_len(98);
    let mut v_no_group_3 = vec![];
    let mut no_group_3 = NoGroups3::default();
    no_group_3.set_pbu_from_string("37167");
    no_group_3.set_set_id(1);
    no_group_3.set_begin_report_index(0);
    no_group_3.set_end_report_index(0);
    no_group_3.set_text_from_string("Success");
    v_no_group_3.push(no_group_3);
    exec_rpt_sync_rsp.set_no_groups_3(&v_no_group_3);
    exec_rpt_sync_rsp.set_checksum(226);
    exec_rpt_sync_rsp.filled_head_and_tail();
    exec_rpt_sync_rsp
};

}

/// TCP 服务器配置
#[derive(Parser, Debug)]
#[command(name = "moc_client")]
#[command(about = "共享报盘moc客户端", long_about = None)]
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


fn genertae_platform_state_msg(platform_id:u16) -> [PlatformState;5] { 
    let mut not_open_state  = PlatformState::default();
    not_open_state.set_platform_id(platform_id);
    not_open_state.set_platform_state(0);
    not_open_state.filled_head_and_tail();

    let mut pre_open_state  = PlatformState::default();
    pre_open_state.set_platform_id(platform_id);
    pre_open_state.set_platform_state(1);
    pre_open_state.filled_head_and_tail();

    let mut open_state  = PlatformState::default();
    open_state.set_platform_id(platform_id);
    open_state.set_platform_state(2);
    open_state.filled_head_and_tail();


    let mut break_state  = PlatformState::default();
    break_state.set_platform_id(platform_id);
    break_state.set_platform_state(3);
    break_state.filled_head_and_tail();


    let mut close_state  = PlatformState::default();
    close_state.set_platform_id(platform_id);
    close_state.set_platform_state(4);
    close_state.filled_head_and_tail();

    [not_open_state,pre_open_state,open_state,break_state,close_state]
}

fn handle_client(mut stream: TcpStream,platform_id:u16) {
    let peer_addr = stream.peer_addr().unwrap();
    println!("\n[新连接] 客户端地址: {}", peer_addr);
    println!("{}", "=".repeat(60));

    let mut buffer = [0u8; 1024];
    let mut msgType: u32 = 0;

    let mut heart_bt_int: u16 = 0;
    let mut last_write_time = Instant::now();
    let mut heartbeat_timeout = Duration::from_secs(30);

    let mut is_logon: bool = false;
    let mut send_cnt: u32 = 0;

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
                println!("send_cnt: {}", send_cnt);

                // 解析 msgType (第0-3字节)
                if n >= 8 {
                    msgType = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
                    println!("msgType: {}", msgType);
                }

                // 如果 msgType == 40，解析 heart_bt_int (第81-82字节)
                if msgType == 40 && n >= 82 {
                    heart_bt_int = u16::from_be_bytes([buffer[80], buffer[81]]);
                    heartbeat_timeout = Duration::from_secs(heart_bt_int as u64); // 心跳超时时间
                    println!("heart_bt_int: {}", heart_bt_int);
                }

                // 回显逻辑
                match msgType {
                    40 => {
                        // 回复登录
                        if let Err(e) = stream.write_all(&data) {
                            eprintln!("发送登录响应失败: {}", e);
                            break;
                        } else {
                            is_logon = true;
                        }
                    }
                    58 =>{
                        let mut new_order = NewOrderSingle::new();
                        new_order.copy_from_big_endian(data);
                        println!("recvie new Order :{:?}",new_order);
                        let mut resp = ExecutionReportResponse::default();
                        resp.set_msg_seq_num(14);
                        resp.set_pbu_from_string(&new_order.get_biz_pbu_as_string().clone());
                        resp.set_set_id(4);
                        resp.set_report_index(1);
                        resp.set_exec_type(56);
                        resp.set_ord_rej_reason(4012);
                        resp.set_user_info(new_order.get_user_info().clone());
                        // 回复ExecutionReportResponse 失败
                        if let Err(e) = stream.write_all(&resp.as_bytes_big_endian().as_slice()) {
                            eprintln!("发送登录响应失败: {}", e);
                            break;
                        } else {
                            is_logon = true;
                            println!("发送拒单响应：{:?}",resp)
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

        if send_cnt == 2 {
            if let Err(e) = stream.write_all(&plateform_state_msgs[2].as_bytes_big_endian().as_slice()) {
                eprintln!("发送平台open失败: {}", e);
                break;
            } else {
                send_cnt += 1;
            }
        }
        if send_cnt == 3 {
            if let Err(e) = stream.write_all(EXEC_RPT_INFO.as_bytes().as_slice()) {
                eprintln!("发送执行报告信息失败: {}", e);
                break;
            } else {
                send_cnt += 1;
            }
        }
        if send_cnt == 4 {
            if let Err(e) = stream.write_all(EXEC_RPT_SYNC_RSP.as_bytes().as_slice()) {
                eprintln!("发送分区序号同步响应: {}", e);
                break;
            } else {
                send_cnt += 1;
            }
        }
        /*
         if send_cnt == 5
        {
            //调整心跳时间为原来的4倍
            println!("超时测试 - 调整心跳时间为原来的4倍");
            heartbeat_timeout = Duration::from_secs(heart_bt_int as u64 * 4);
        }
        
        if send_cnt == 6 {
            let mut logout = Logout::new();
            logout.set_text_from_string("tdgw logout");
            logout.set_session_status(0);
            logout.filled_head_and_tail();

            if let Err(e) = stream.write_all(logout.as_bytes_big_endian().as_slice()) {
                eprintln!("发送登出请求: {}", e);
                break;
            } else {
                send_cnt += 1;
                //打印发送出的LOGOUT buf的十六进制形式
                println!("发送LOGOUT, {:?}",logout);
            }
        }

        
        if send_cnt ==8
        {
            println!("关闭连接");
            if let Err(e) = stream.shutdown(std::net::Shutdown::Both) {
                eprintln!("关闭连接失败: {}", e);
            }
            break
        }
        */
        println!(
            "心跳检查：is_logon={}, 距上次发送耗时={:?}, 心跳间隔={:?}",
            is_logon,
            last_write_time.elapsed().as_secs_f64(),
            heartbeat_timeout.as_secs_f64()
        );

        if is_logon && last_write_time.elapsed() > heartbeat_timeout {
            println!("触发心跳，当前发送计数数: {}", send_cnt + 1);
            match stream.write_all(TDGW_HEARTHB_MSG.as_bytes_big_endian().as_slice()) {
                Ok(_) => {
                    send_cnt += 1;
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
    println!("  服务器已启动");
    println!("  监听地址: {},平台ID：{}", addr,platform_id);
    println!("====================================");
    println!("\n等待客户端连接...\n");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    handle_client(stream,platform_id);
                });
            }
            Err(e) => {
                eprintln!("连接失败: {}", e);
            }
        }
    }
}
