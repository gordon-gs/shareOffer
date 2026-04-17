/**
 * @file     : tcp_conn_type.h
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 21:46:46
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#ifndef TCP_CONN_TYPE_H
#define TCP_CONN_TYPE_H

#include <stddef.h>
#include <stdint.h>
#include <errno.h>

typedef enum
{
    TCP_TYPE_SOCKET = 0,  ///< tcp_mgr_type: socket
    TCP_TYPE_INSTANTA,    ///< tcp_mgr_type: instanta
    TCP_TYPE_NDPP_TOE     ///< tcp_mgr_type: ndpp_toe
} tcp_conn_manage_type_t;

typedef enum
{
    CONN_TYPE_UNKNOWN = 0,
    CONN_TYPE_CLIENT,
    CONN_TYPE_SERVER,
    CONN_TYPE_ACCEPT_WORKER  ///< 内部使用
} conn_type_t;

typedef enum
{
    CONN_STATE_NONE = 0,
    CONN_STATE_CONNECTING,
    CONN_STATE_LISTENING,
    CONN_STATE_CONNECTED,
    CONN_STATE_CLOSING,  // 半关闭(视图写数据会返回失败)
    CONN_STATE_CLOSED
} conn_state_t;

typedef enum
{
    TCP_EVENT_NONE      = 0,
    TCP_EVENT_CONNECTED = 1,
    TCP_EVENT_RX_READY  = 2,
    TCP_EVENT_TX_READY  = 3,
    TCP_EVENT_CLOSING   = 4,  ///< 半关闭(通知不要写入,不一定触发)
    TCP_EVENT_CLOSED    = 5,
    TCP_EVENT_ERROR     = 6,  ///< 连接异常（系统错误、RST、超时等）
} conn_event_type_t;

typedef enum
{
    TCP_OP_NONE = 0,
    TCP_OP_SOCKET,
    TCP_OP_BIND,
    TCP_OP_LISTEN,
    TCP_OP_ACCEPT,
    TCP_OP_CONNECT,
    TCP_OP_READ,
    TCP_OP_WRITE,
    TCP_OP_CLOSE,
    TCP_OP_EPOLL,
    TCP_OP_FCNTL
} tcp_socket_op_t;

typedef struct
{
    uint16_t conn_id;
    uint8_t  type;
    uint8_t  resv;        ///<- tcp_socket_op_t : socket op causing the event
    int32_t  error_code;
} tcp_conn_event_t;

struct tcp_conn_info_s
{
    uint16_t    conn_id;
    uint32_t    route_id;       ///< Route ID for routing purposes
    char        conn_tag[32];

    conn_type_t conn_type;      ///< tcp server or tcp client
    char        local_mac[32];  ///<
    char        local_ip[16];
    int         local_port;
    char        remote_mac[32];
    char        remote_ip[16];
    int         remote_port;

    // 新增字段
    char        bind_interface[32];  ///< 绑定的网络接口名称
    int         max_retries;         ///< 最大重试次数
    int         retry_interval_ms;   ///< 重试间隔（毫秒）

    int         rx_pipe_fd;
    int         tx_pipe_fd;
    int         rx_buffer_used;  // RX buffer usage
    int         rx_buffer_size;  // RX buffer total size
    int         tx_buffer_used;  // TX buffer usage
    int         tx_buffer_size;  // TX buffer total size

    int              conn_enabled;  ///< 配置禁用或者无效
    int              single_shot;   ///< 是否为单次连接
    conn_state_t     conn_state;    ///< 连接状态
    struct tcp_conn_stats_s *stats; ///< 统计信息
};

typedef struct tcp_global_settings_s
{
    int max_connections;
    int ring_buffer_size;
    int reconnect_interval;
    int connection_timeout;
    int rx_thread_cpu_id;  ///< rx_thread_func 绑定的 CPU core ID，-1 表示不绑定
} tcp_global_settings_t;

typedef struct
{
    char host_ip[32];
    char host_mac[32];        ///< 为空或不配置, 则根据host_ip自动解析生成
    int  is_local;            ///< 预留.是否为本地主机(1=本地, 0=远程)
} arp_entry_t;

typedef struct tcp_conn_mgr_settings_s
{
    char                   version[16];
    char                   description[128];
    char                   type[16];
    char                   log_config_path[128];
    tcp_global_settings_t  global;
    int                    arp_count;
    arp_entry_t            arp_table[32];  // 可根据最大ARP数量调整
    int                    conn_count;
    struct tcp_conn_info_s conn_list[128];  // 对应 max_connections 限制
} tcp_conn_mgr_settings_t;

struct tcp_conn_stats_s
{
    /* ---------- 外部统计（应用可见） ---------- */
    uint64_t bytes_sent;        ///< 发送到远端的字节数（兼容性字段）
    uint64_t bytes_received;    ///< 从远端接收到并交付上层的字节数（兼容性字段）
    uint64_t packets_sent;      ///< 成功发送的数据包数（兼容性字段）
    uint64_t packets_received;  ///< 接收到并处理的数据包数（兼容性字段）
    uint64_t reconnect_count;   ///< 重连次数
    uint64_t last_active_time;  ///< 最后活动时间

#if 0
    /* ---------- 内部转发统计 ---------- */
    uint64_t tx_bytes_forwarded;  ///< 内部转发发送的字节数（例如代理模式）
    uint64_t rx_bytes_forwarded;  ///< 内部转发接收的字节数
    uint64_t tx_forward_drops;    ///< 转发过程中丢弃的包数（缓冲区溢出/异常）
    uint64_t rx_forward_drops;    ///< 接收转发丢弃的包数
    /* ---------- 缓冲与队列统计 ---------- */
    uint64_t tx_queue_bytes;  ///< 发送缓冲区中的字节数
    uint64_t tx_queue_peak;   ///< 发送队列峰值
    uint64_t rx_queue_bytes;  ///< 接收缓冲区中的字节数
    uint64_t rx_queue_peak;   ///< 接收缓冲区峰值
    /* ---------- 错误与异常 ---------- */
    uint64_t tx_errors;  ///< 发送错误计数（send() < 0）
    uint64_t rx_errors;  ///< 接收错误计数（recv() < 0）
    // uint64_t retransmissions;    ///< TCP 层重传次数（如可获取）
    // uint64_t connection_resets;  ///< 对端复位次数（RST）
    /* ---------- 延迟与时间统计 ---------- */
    // uint64_t rtt_avg_ms;        ///< 平均往返时延（可选）
    uint64_t last_activity_ms;  ///< 最近一次数据活动时间戳
    uint64_t uptime_ms;         ///< 连接已持续时间
    /* ---------- 状态监控 ---------- */
    uint64_t events_triggered;   ///< 事件触发总数（回调次数）
    uint64_t state_transitions;  ///< 状态转换次数（CONNECT->CLOSED等）
#endif
};

/* 辅助函数：枚举转字符串 */
static const char *conn_type_str(int type) __attribute__((unused));
static const char *conn_type_str(int type)
{
    switch (type)
    {
    case CONN_TYPE_SERVER:
        return "Server";
    case CONN_TYPE_CLIENT:
        return "Client";
    default:
        return "Unknown";
    }
}

/* 辅助函数：枚举转字符串 */
static const char *conn_state_str(int state) __attribute__((unused));
static const char *conn_state_str(int state)
{
    switch (state)
    {
    case CONN_STATE_NONE:
        return "None";
    case CONN_STATE_CONNECTING:
        return "Connecting";
    case CONN_STATE_LISTENING:
        return "Listening";
    case CONN_STATE_CONNECTED:
        return "Connected";
    case CONN_STATE_CLOSING:
        return "Closing";
    case CONN_STATE_CLOSED:
        return "Closed";
    default:
        return "Unknown";
    }
}

/* 辅助函数：连接事件类型枚举转字符串 */
static const char *conn_event_type_str(int type) __attribute__((unused));
static const char *conn_event_type_str(int type)
{
    switch ((conn_event_type_t)type)
    {
    case TCP_EVENT_NONE:
        return "Nothing";
    case TCP_EVENT_CONNECTED:
        return "Connected";
    case TCP_EVENT_RX_READY:
        return "RX Data Ready";
    case TCP_EVENT_TX_READY:
        return "TX Data Ready";
    case TCP_EVENT_CLOSING:
        return "Closing";
    case TCP_EVENT_CLOSED:
        return "Closed";
    case TCP_EVENT_ERROR:
        return "Error";
    default:
        return "Unknown";
    }
}

#endif  // TCP_CONN_TYPE_H
