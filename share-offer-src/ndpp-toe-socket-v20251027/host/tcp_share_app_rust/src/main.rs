mod bindings;

use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;
use std::thread;
use std::time::Duration;

use bindings::*;
use libc::{epoll_create1, epoll_ctl, epoll_event, epoll_wait, read, EPOLLIN, EPOLL_CTL_ADD};

const MAX_EVENTS: usize = 32;

static mut TCP_CONN_LIST: [*mut tcp_conn_item_t; 2] = [ptr::null_mut(); 2];
static mut G_MGR: *mut tcp_conn_manage_t = ptr::null_mut();

unsafe fn tcp_forward_app_thread() {
    let epfd = epoll_create1(0);
    let mut ev: epoll_event = std::mem::zeroed();

    for i in 0..2 {
        let conn = TCP_CONN_LIST[i];
        if conn.is_null() {
            continue;
        }

        let rx_pipe_fd = tcp_conn_get_event_fd(conn);
        if rx_pipe_fd < 0 {
            continue;
        }

        ev.u64 = conn as u64;
        ev.events = EPOLLIN as u32;
        epoll_ctl(epfd, EPOLL_CTL_ADD, rx_pipe_fd, &mut ev);
    }

    let mut events: [epoll_event; MAX_EVENTS] = [std::mem::zeroed(); MAX_EVENTS];

    loop {
        let n = epoll_wait(epfd, events.as_mut_ptr(), MAX_EVENTS as i32, -1);
        for i in 0..n as usize {
            let conn = events[i].u64 as *mut tcp_conn_item_t;
            let rx_pipe_fd = tcp_conn_get_event_fd(conn);

            let mut evt: tcp_conn_event_t = std::mem::zeroed();
            read(rx_pipe_fd, &mut evt as *mut _ as *mut c_void, std::mem::size_of::<tcp_conn_event_t>());

            match evt.type_ {
                conn_event_type_t::TCP_EVENT_RX_READY => {
                    let mut data: *const c_void = ptr::null();
                    let mut len: i32 = 0;
                    if tcp_conn_recv(conn, &mut data, &mut len) == 0 && len > 0 {
                        println!("[app] ConnID {} received {} bytes.", evt.conn_id, len);

                        let src_id = evt.conn_id as usize;
                        let dst_id = if src_id == 0 { 1 } else { 0 };

                        let dst_conn = tcp_conn_find_by_id(G_MGR, dst_id as u16);
                        if !dst_conn.is_null() && tcp_conn_state(dst_conn) == conn_state_t::CONN_STATE_CONNECTED as i32 {
                            println!("[app] ConnID {} try forward {} bytes.", dst_id, len);
                            tcp_conn_send(dst_conn, data, len);
                        }

                        tcp_conn_consume(conn, len);
                    }
                }
                conn_event_type_t::TCP_EVENT_CLOSED => {
                    println!("[app] ConnID {} closed.", evt.conn_id);
                }
                _ => {}
            }
        }
    }
}

unsafe fn load_tcp_channel_config() {
    let mut server1 = tcp_server_config_t {
        listen_port: 18000,
        listen_ip: [0; 16],
        max_clients: 1,
        num_configs: 1,
        client_configs: tcp_client_config_t {
            remote_port: 0,
            remote_ip: [0; 16],
        },
    };

    let server_conn = tcp_conn_listen(G_MGR, &mut server1);

    let mut client1 = tcp_client_config_t {
        remote_port: 15201,
        remote_ip: [0; 16],
    };

    // Copy IP address string
    let ip_str = CString::new("192.168.56.1").unwrap();
    let ip_bytes = ip_str.as_bytes_with_nul();
    for (i, &byte) in ip_bytes.iter().enumerate() {
        if i >= 16 { break; }
        client1.remote_ip[i] = byte as i8;
    }

    let client_conn = tcp_conn_connect(G_MGR, &mut client1);

    TCP_CONN_LIST[0] = tcp_conn_find_by_id(G_MGR, server_conn as u16);
    TCP_CONN_LIST[1] = tcp_conn_find_by_id(G_MGR, client_conn as u16);
}

fn main() {
    unsafe {
        let config_path = CString::new("../utest/tcp_conn_config.json").unwrap();
        G_MGR = tcp_conn_mgr_create(config_path.as_ptr());

        load_tcp_channel_config();

        let _ = thread::spawn(|| {
            unsafe { tcp_forward_app_thread(); }
        });

        println!(
            "TCP Forward Relay: tcp_conn_0(::18000) <--> tcp_conn_1(192.168.56.1:15201)"
        );

        loop {
            thread::sleep(Duration::from_secs(3600));
        }

    }
}
