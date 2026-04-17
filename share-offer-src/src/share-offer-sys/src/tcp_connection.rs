#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]

use crate::{tcp_connection, tcp_error, utils};
use fproto::protocol_error::ProtocolError;
use fproto::stream_frame::StreamFrame;
use fproto::FrameResult;
use libc::{
    epoll_create1, epoll_ctl, epoll_event, epoll_wait, read, sigaction, sigemptyset, stat, EPOLLIN,
    EPOLL_CTL_ADD, SA_RESTART, SIGINT,
};
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use std::sync::Arc;
use std::vec;
use std::{ffi::c_void, string};
use tracing::{debug, error};

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[derive(Clone, Debug, Default, Copy)]
pub struct TCPConnection {
    pub conn: *mut tcp_conn_item_t,
}

unsafe impl Sync for TCPConnection {}
unsafe impl Send for TCPConnection {}

pub struct TCPConnectionManager {
    pub mgr: *mut tcp_connection::tcp_conn_manage_t,
}

impl TCPConnectionManager {
    pub fn find_conn_by_routing(&self, id: u16) -> TCPConnection {
        tcp_connection::get_tcp_connection(self.mgr, id)
    }
}

unsafe impl Sync for TCPConnectionManager {}
unsafe impl Send for TCPConnectionManager {}

pub struct TCPEventEpoll {
    fd: libc::c_int,
    // wake_fd: i32,
}

pub struct TCPConnectionInfo {
    pub info_ptr: *const tcp_conn_info_s,
}

// extern "C" fn signal_handler(_sig: i32) {
//     println!("\n epoll 收到退出信号");
// }

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum ConnectionProcessState {
    Normal,
    Error,
}

struct RecvData<'a> {
    ptr: *const c_void,             // 原始数据指针（FFI 返回）
    len: i32,                       // 数据长度（字节数）
    _marker: PhantomData<&'a [u8]>, // 绑定生命周期，确保切片安全
}
impl TCPEventEpoll {
    pub fn new() -> Self {
        unsafe {
            // let mut sa: libc::sigaction = mem::zeroed();
            // sa.sa_sigaction = signal_handler as usize;
            // sigemptyset(&mut sa.sa_mask);
            // sa.sa_flags = SA_RESTART;
            // if sigaction(SIGINT, &sa, std::ptr::null_mut()) < 0{
            //     panic!("设置hanlde 信号量错误")
            // }

            let epoll_fd = libc::epoll_create1(0);
            if epoll_fd < 0 {
                panic!("Failed to create epoll fd");
            }

            // let wake_fd = libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC);
            // if wake_fd < 0 {
            //     panic!("Failed to create eventfd");
            // }

            let epoll_event = Self {
                fd: epoll_fd,
                // wake_fd,
            };

            // 注册 wake_fd 到 epoll 监听
            // let mut send_ev: libc::epoll_event = std::mem::zeroed();
            // send_ev.u64 = epoll_event.wake_fd as u64;
            // send_ev.events = libc::EPOLLIN as u32;

            // if libc::epoll_ctl(
            //     epoll_event.fd,
            //     libc::EPOLL_CTL_ADD,
            //     epoll_event.wake_fd,
            //     &mut send_ev,
            // ) < 0
            // {
            //     panic!("Failed to add eventfd to epoll");
            // }

            epoll_event
        }
    }

    pub fn add_connection(&mut self, conn: &TCPConnection) -> FrameResult<()> {
        unsafe {
            let rx_pipe_fd = tcp_conn_get_event_fd(conn.conn);
            utils::test_share_offer_rc(rx_pipe_fd, "add connection")?;
            let mut ev: epoll_event = std::mem::zeroed();
            ev.u64 = conn.conn as u64;
            ev.events = EPOLLIN as u32;
            let ret = epoll_ctl(self.fd, EPOLL_CTL_ADD, rx_pipe_fd, &mut ev);
            if ret < 0 {
                let err = std::io::Error::last_os_error();
                error!(
                    "stream add epoll error, error: {:?}, socket: {:?}",
                    err, conn
                );
            }
        }
        Ok(())
    }

    #[inline(always)]
    pub fn get_ready_events(&mut self) -> Vec<TCPConnection> {
        unsafe {
            let mut events: [epoll_event; 1024] = [std::mem::zeroed(); 1024];
            let n = if !cfg!(feature = "local_debug") {
                epoll_wait(self.fd, events.as_mut_ptr(), 1024, 0)
            } else {
                epoll_wait(self.fd, events.as_mut_ptr(), 1024, 100)
            };
            let mut result = Vec::with_capacity(n as usize);
            if n >= 0 {
                for i in 0..n as usize {
                    // if events[i as usize].u64 as i32 == self.wake_fd {
                    //     let mut buf = 0u64;
                    //     libc::read(self.wake_fd, &mut buf as *mut _ as *mut libc::c_void, 8);
                    //     //debug!("receive send msg fd begin break epoll loop !")
                    // } else {

                    // }
                    let conn = events[i].u64 as *mut tcp_conn_item_t;
                    result.push(TCPConnection { conn });
                }
            } else {
                // let err = std::io::Error::last_os_error();
                // eprintln!("epoll_wait 错误: {}", err);
                //pass
            }
            result
        }
    }

    // pub fn send_wakeup(&self) {
    //     let val: u64 = 1;
    //     unsafe {
    //         // 注意：必须发送 8 字节， eventfd 的规范
    //         let n = libc::write(self.wake_fd, &val as *const u64 as *const libc::c_void, 8);
    //         if n < 0 {
    //             // 如果是 EWOULDBLOCK 说明缓冲区满了，但在这种唤醒场景下通常可以忽略
    //             // 因为缓冲区满了也意味着 epoll 肯定会被唤醒
    //             let err = std::io::Error::last_os_error();
    //         } else {
    //         }
    //     }
    // }
}

impl TCPConnection {
    #[inline(always)]
    pub fn get_event_fd(&self) -> i32 {
        unsafe { tcp_conn_get_event_fd(self.conn) }
    }

    #[inline(always)]
    fn recv_zc(&self) -> RecvData<'_> {
        unsafe {
            let mut data = RecvData {
                ptr: ptr::null_mut(),
                len: 0,
                _marker: PhantomData,
            };
            tcp_conn_recv(self.conn, &mut data.ptr, &mut data.len);
            data
        }
    }
    #[inline(always)]
    fn tcp_conn_consume(&self, len: i32) -> i32 {
        unsafe { tcp_conn_consume(self.conn, len) }
    }

    #[inline(always)]
    pub fn parse_frame<T: StreamFrame>(
        &mut self,
        state: &ConnectionProcessState,
    ) -> FrameResult<Option<Arc<T>>> {
        let recv_data = self.recv_zc();
        if recv_data.len == 0 {
            return Ok(None);
        }
        let recv_data_ref = recv_data.as_slice();

        match <T as StreamFrame>::check(recv_data_ref) {
            Ok((msg_type, body_length)) => {
                let frame = match <T as StreamFrame>::parse_as_ref(recv_data_ref, msg_type) {
                    Ok(f) => f,
                    Err(error) => {
                        //self.tcp_conn_consume(body_length as i32);
                        return match error {
                            ProtocolError::UnImplemented(msg_type) => {
                                error!(
                                    "receive unsupported message, msg_type: {}, socket: {:?}",
                                    msg_type, self.conn
                                );
                                // Ok(Some(Arc::new(<T as StreamFrame>::skip_frame())))
                                Err(error)
                            }
                            _ => Ok(None),
                        };
                    }
                };
                self.tcp_conn_consume(body_length as i32);
                //debug!("business::frame buffer check STEP 5: conn:{:?},len:{:?}",self.conn,recv_data.len);
                Ok(Some(frame))
            }
            Err(error) => {
                //debug!("business::frame buffer check STEP 6: conn:{:?},len:{:?}",self.conn,recv_data.len);
                match error {
                    ProtocolError::Incomplete => {
                        if *state == ConnectionProcessState::Error {
                            error!(
                            "connection error find incomplete message socket: {:?},buffer size:{:?}, clear buff",
                            self.conn,recv_data.len
                        );
                            self.tcp_conn_consume(recv_data.len);
                        }
                        Ok(None)
                    }
                    ProtocolError::IncompleteDetail(
                        msg_type,
                        msg_seq_num,
                        bodyLength,
                        buffer_size,
                    ) => {
                        if *state == ConnectionProcessState::Error {
                            error!(
                            "connection error find incomplete message, msg_type: {},msg_seq_num:{},body_length:{},buffer_size:{}, socket: {:?}, clear buff",
                            msg_type,msg_seq_num,bodyLength,buffer_size,self.conn
                        );
                            self.tcp_conn_consume(recv_data.len);
                        }

                        Ok(None)
                    }
                    ProtocolError::UnImplemented(msg_type) => {
                        error!(
                            "receive unsupported message, msg_type: {}, socket: {:?}",
                            msg_type, self.conn
                        );
                        Ok(None)
                    }
                    // should not happen for now
                    _ => {
                        error!(
                            "stream check error, error: {:?}, socket: {:?}",
                            error, self.conn
                        );
                        Err(error)
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub fn tcp_conn_send_zc<T: ?Sized>(&self, data: &T, data_size: i32) -> FrameResult<()> {
        // 1. 计算数据结构的大小（使用 std::mem::size_of，与 C 的 sizeof 一致）

        // 2. 将数据结构体转换为 *const c_void 指针（安全：data 是有效引用，生命周期由调用者保证）
        let data_ptr = data as *const T as *const c_void;
        let ret = unsafe { tcp_conn_send(self.conn, data_ptr, data_size as i32) };
        utils::test_share_offer_rc(ret, "tcp_conn_send")
    }

    #[inline(always)]
    pub fn tcp_conn_send_bytes(&self, data: &Vec<u8>) -> FrameResult<()> {
        // 1. 计算数据结构的大小（使用 std::mem::size_of，与 C 的 sizeof 一致）

        // 2. 将数据结构体转换为 *const c_void 指针（安全：data 是有效引用，生命周期由调用者保证）
        let data_ptr = data.as_ptr() as *const c_void;

        let ret = unsafe { tcp_conn_send(self.conn, data_ptr, data.len() as i32) };
        utils::test_share_offer_rc(
            ret,
            &format!("tcp_conn_send:{}", tcp_error::TCPLibcError::from(ret)),
        )
    }

    #[inline(always)]
    pub fn write_frame<T: StreamFrame>(&self, frame: &mut T) -> FrameResult<()> {
        <T as StreamFrame>::perpare_for_send(frame)?;
        let msg = <T as StreamFrame>::serialize(&frame)?;
        self.tcp_conn_send_bytes(&msg)
    }

    #[inline(always)]
    pub fn tcp_get_conn_info(&self) -> TCPConnectionInfo {
        unsafe {
            TCPConnectionInfo {
                info_ptr: tcp_conn_get_info(self.conn),
            }
        }
    }

    pub fn tcp_conn_close(&self) -> FrameResult<()> {
        unsafe {
            let ret = tcp_conn_close(self.conn);
            utils::test_share_offer_rc(ret, "tcp_conn_close")
        }
    }

    pub fn tcp_conn_reset(&self) -> FrameResult<()> {
        unsafe {
            let ret = tcp_conn_reset(self.conn);
            utils::test_share_offer_rc(ret, "tcp_conn_reset")
        }
    }
}

impl TCPConnectionInfo {
    pub fn get_conn_id(&self) -> u16 {
        unsafe {
            let info = &*self.info_ptr;
            info.conn_id
        }
    }

    pub fn get_local_ip(&self) -> String {
        unsafe {
            let info = &*self.info_ptr;
            CStr::from_ptr(info.local_ip.as_ptr())
                .to_string_lossy()
                .to_string()
        }
    }

    pub fn get_local_port(&self) -> u16 {
        unsafe {
            let info = &*self.info_ptr;
            info.local_port as u16
        }
    }

    pub fn get_remote_ip(&self) -> String {
        unsafe {
            let info = &*self.info_ptr;
            CStr::from_ptr(info.remote_ip.as_ptr())
                .to_string_lossy()
                .to_string()
        }
    }

    pub fn get_remote_port(&self) -> u16 {
        unsafe {
            let info = &*self.info_ptr;
            info.remote_port as u16
        }
    }

    pub fn get_conn_state(&self) -> conn_state_t {
        unsafe {
            let info = &*self.info_ptr;
            info.conn_state
        }
    }

    // pub fn get_remote_ip(&self) -> String{
    //     unsafe{

    //     }
    // }
}

impl<'a> RecvData<'a> {
    fn read_u32_from(&self, offset: usize) -> u32 {
        // 计算起始指针位置（从 m 字节开始）
        let start_ptr = unsafe { self.ptr.add(offset) as *const u8 };

        // 读取 4 字节到切片（安全，因已通过边界检查）
        let bytes = unsafe { std::slice::from_raw_parts(start_ptr, 4) };

        // 转换为 u32（假设数据是大端字节序，如网络传输场景）
        // 若为小端字节序，使用 from_le_bytes()
        let u32_val = u32::from_be_bytes(bytes.try_into().unwrap()); // try_into 在此安全（长度固定为 4）

        u32_val
    }

    /// 返回数据的字节切片 &[u8]（安全访问）
    ///
    /// # 安全要求
    /// - 调用期间 `self.ptr` 指向的内存必须有效（未被释放）
    pub fn as_slice(&self) -> &[u8] {
        // 安全：已通过构造函数检查指针非空，且 len 是数据实际长度
        unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.len as usize) }
    }
}

#[inline(always)]
pub fn read_fd_event(fd: i32) -> tcp_conn_event_t {
    unsafe {
        let mut evt: tcp_conn_event_t = std::mem::zeroed();
        read(
            fd,
            &mut evt as *mut _ as *mut c_void,
            std::mem::size_of::<tcp_conn_event_t>(),
        );
        evt
    }
}

#[inline(always)]
pub fn get_tcp_connection(tcp_mgr: *mut tcp_conn_manage_t, conn_id: u16) -> TCPConnection {
    unsafe {
        TCPConnection {
            conn: tcp_conn_find_by_id(tcp_mgr, conn_id),
        }
    }
}
