pub mod tcp_connection;
pub mod tcp_error;
mod utils;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
    use super::*;
    use std::ffi::CString;
    use std::os::raw::c_void;
    use std::ptr;
    use std::thread;
    use std::time::Duration;

    use libc::{epoll_create1, epoll_ctl, epoll_event, epoll_wait, read, EPOLLIN, EPOLL_CTL_ADD};

    const MAX_EVENTS: usize = 32;

    static mut TCP_CONN_LIST: [*mut tcp_conn_item_t; 2] = [ptr::null_mut(); 2];
    static mut G_MGR: *mut tcp_conn_manage_t = ptr::null_mut();

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }

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
                read(
                    rx_pipe_fd,
                    &mut evt as *mut _ as *mut c_void,
                    std::mem::size_of::<tcp_conn_event_t>(),
                );

                match evt.type_ {
                    x if x == conn_event_type_t_TCP_EVENT_RX_READY as u8 => {
                        let mut data: *const c_void = ptr::null();
                        let mut len: i32 = 0;
                        if tcp_conn_recv(conn, &mut data, &mut len) == 0 && len > 0 {
                            println!("[main] ConnID {} received {} bytes.", evt.conn_id, len);

                            let src_id = evt.conn_id as usize;
                            let dst_id = if src_id == 0 { 1 } else { 0 };

                            let dst_conn = tcp_conn_find_by_id(G_MGR, dst_id as i32);
                            if !dst_conn.is_null()
                                && tcp_conn_state(dst_conn)
                                    == conn_state_t_CONN_STATE_CONNECTED as i32
                            {
                                println!("[main] ConnID {} try forward {} bytes.", dst_id, len);
                                tcp_conn_send(dst_conn, data, len);
                            }

                            tcp_conn_consume(conn, len);
                        }
                    }
                    x if x == conn_event_type_t_TCP_EVENT_CLOSED as u8 => {
                        println!("[main] ConnID {} closed.", evt.conn_id);
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn it_works_2() {
        let config_file = CString::new("config/tcp_share_config.json").unwrap();


        unsafe {
            G_MGR = tcp_conn_mgr_create(config_file.as_ptr());

            println!("tcp_conn_mgr_create returned: {:p}", G_MGR);

            if G_MGR.is_null() {
                println!(
                    "Error: Failed to create TCP connection manager with config file:{}",
                    "config/tcp_share_config.json"
                );
                std::process::exit(1);
            } else {
                println!("Successfully created TCP connection manager");
            }

            load_tcp_channel_config();

            tcp_conn_mgr_start(G_MGR);

            let _ = thread::spawn(|| unsafe {
                tcp_forward_app_thread();
            });

            println!("TCP Frowad Relay: tcp_conn_0(::18000) <--> tcp_conn_1(192.168.56.1:15201)");

            loop {
                thread::sleep(Duration::from_secs(3600));
            }
        }
    }
}
