use libc;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TCPLibcError {
    // --- 1. 网络连接相关 ---
    ConnectionRefused,    // ECONNREFUSED: 目标拒绝连接
    ConnectionReset,      // ECONNRESET: 连接被对方重置 (RST)
    ConnectionAborted,    // ECONNABORTED: 软件导致的连接中止
    TimedOut,             // ETIMEDOUT: 操作超时
    NotConnected,         // ENOTCONN: 套接字未连接
    IsConnected,          // EISCONN: 套接字已连接
    AddrInUse,            // EADDRINUSE: 地址/端口已被占用
    AddrNotAvailable,     // EADDRNOTAVAIL: 无法分配请求的地址
    NetUnreachable,       // ENETUNREACH: 网络不可达
    HostUnreachable,      // EHOSTUNREACH: 主机不可达
    InProgress,           // EINPROGRESS: 操作正在进行中（非阻塞 connect）
    AlreadyInProgress,    // EALREADY: 操作已经在进行中

    // --- 2. 资源与阻塞相关 ---
    WouldBlock,           // EAGAIN / EWOULDBLOCK: 资源暂时不可用（请重试）
    Interrupted,          // EINTR: 系统调用被信号中断（通常需重试）
    OutOfMemory,          // ENOMEM: 内存不足
    NoBufferSpace,        // ENOBUFS: 缓冲队列已满
    TooManyOpenFiles,     // EMFILE / ENFILE: 打开的文件描述符过多

    // --- 3. 参数与权限相关 ---
    PermissionDenied,     // EACCES / EPERM: 权限不足
    InvalidArgument,      // EINVAL: 无效参数
    BadFileDescriptor,    // EBADF: 错误的文件描述符
    PipeClosed,           // EPIPE: 管道破裂（对端已关闭读）
    NotASocket,           // ENOTSOCK: 对非套接字进行套接字操作
    MessageTooLong,       // EMSGSIZE: 消息过长

    // --- 4. 文件系统相关 ---
    NotFound,             // ENOENT: 文件或目录不存在
    AlreadyExists,        // EEXIST: 文件已存在
    IsADirectory,         // EISDIR: 是一个目录
    NotADirectory,        // ENOTDIR: 不是一个目录
    NotEmpty,             // ENOTEMPTY: 目录不为空

    // --- 5. 其他 ---
    Unknown(i32),         // 未知错误
}

impl From<i32> for TCPLibcError {
    fn from(errno: i32) -> Self {
        match errno {
            libc::ECONNREFUSED => TCPLibcError::ConnectionRefused,
            libc::ECONNRESET => TCPLibcError::ConnectionReset,
            libc::ECONNABORTED => TCPLibcError::ConnectionAborted,
            libc::ETIMEDOUT => TCPLibcError::TimedOut,
            libc::ENOTCONN => TCPLibcError::NotConnected,
            libc::EISCONN => TCPLibcError::IsConnected,
            libc::EADDRINUSE => TCPLibcError::AddrInUse,
            libc::EADDRNOTAVAIL => TCPLibcError::AddrNotAvailable,
            libc::ENETUNREACH => TCPLibcError::NetUnreachable,
            libc::EHOSTUNREACH => TCPLibcError::HostUnreachable,
            libc::EINPROGRESS => TCPLibcError::InProgress,
            libc::EALREADY => TCPLibcError::AlreadyInProgress,

            libc::EAGAIN | libc::EWOULDBLOCK => TCPLibcError::WouldBlock,
            libc::EINTR => TCPLibcError::Interrupted,
            libc::ENOMEM => TCPLibcError::OutOfMemory,
            libc::ENOBUFS => TCPLibcError::NoBufferSpace,
            libc::EMFILE | libc::ENFILE => TCPLibcError::TooManyOpenFiles,

            libc::EACCES | libc::EPERM => TCPLibcError::PermissionDenied,
            libc::EINVAL => TCPLibcError::InvalidArgument,
            libc::EBADF => TCPLibcError::BadFileDescriptor,
            libc::EPIPE => TCPLibcError::PipeClosed,
            libc::ENOTSOCK => TCPLibcError::NotASocket,
            libc::EMSGSIZE => TCPLibcError::MessageTooLong,

            libc::ENOENT => TCPLibcError::NotFound,
            libc::EEXIST => TCPLibcError::AlreadyExists,
            libc::EISDIR => TCPLibcError::IsADirectory,
            libc::ENOTDIR => TCPLibcError::NotADirectory,
            libc::ENOTEMPTY => TCPLibcError::NotEmpty,

            _ => TCPLibcError::Unknown(errno),
        }
    }
}

impl fmt::Display for TCPLibcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}