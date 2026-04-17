/**
 * @file     : tcp_conn_common.h
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 16:41:38
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#pragma once
#include "tcp_conn.h"
#include "tcp_buffer_rx_tx.h"
#include "ts_list.h"
#include "stdafx.h"

#include "tcp_conn_define.h"
#include "tcp_conn_version.h"
#include "tcp_conn_type.h"

struct tcp_conn_item_s;
struct tcp_conn_manage_s;
struct tcp_conn_listner_s;

typedef struct
{
    int  remote_port;
    char remote_ip[16];
} tcp_client_config_t;

typedef struct
{
    int                 listen_port;
    char                listen_ip[16];
    int                 max_clients;
    int                 num_configs;
    tcp_client_config_t client_configs;
} tcp_server_config_t;

typedef struct tcp_conn_listner_s
{
    struct tcp_conn_manage_s *mgr;
    conn_type_t               type;
    uint16_t                  conn_id;
    conn_state_t              state;
    void                     *userdata;
    tcp_conn_item_t          *accept_conn;

    // 监听配置信息（延迟监听时使用）
    char                      local_ip[16];
    uint16_t                  local_port;
} tcp_conn_listner_t;

typedef struct tcp_conn_remote_s
{
    struct tcp_conn_manage_s *mgr;  // eg.
    conn_type_t               type;
    uint16_t                  conn_id;
    conn_state_t              state;
    void                     *userdata;
} tcp_conn_remote_t;

struct tcp_conn_item_s
{
    tcp_conn_manage_t        *mgr;
    uint16_t                  conn_id;
    struct tcp_conn_info_s   *cfg;   ///< 连接配置结构
    conn_type_t               type;  /// client or accepted_client

    conn_state_t              state;
    rx_buffer_t               rx_buf;
    tx_buffer_t               tx_buf;
    tcp_conn_listner_t       *listner;
    // tcp_conn_remote_t        *remote;
    time_t                    last_active;

    //! 重连和异常处理
    int      io_fd;
    char     remote_ip[16];      // IPv4 address
    uint16_t remote_port;        // Remote port
    uint8_t  retry_count;        // Current retry count
    uint8_t  max_retries;        // Max retry attempts
    uint32_t retry_interval_ms;  // Retry interval in milliseconds

    //! 网络配置
    char     local_ip[16];       // Local IP address
    uint16_t local_port;         // Local port
    char     bind_interface[32]; // Bind interface name

    //! 统计信息
    struct tcp_conn_stats_s stats;
};

// 前向声明
typedef struct tcp_conn_manage_ops_s    tcp_conn_manage_ops_t;
typedef struct tcp_conn_event_handler_s tcp_conn_event_handler_t;

// 事件处理器结构
struct tcp_conn_event_handler_s
{
    void (*on_connected)(tcp_conn_item_t *conn, void *userdata);     ///< 连接建立事件
    void (*on_disconnected)(tcp_conn_item_t *conn, void *userdata);  ///< 连接断开事件
    void (*on_data_received)(tcp_conn_item_t *conn, const void *data, size_t len, void *userdata);  ///< 数据接收事件
    void (*on_data_sent)(tcp_conn_item_t *conn, size_t len, void *userdata);  ///< 数据发送事件
    void (*on_error)(tcp_conn_item_t *conn, int error_code, const char *error_msg, void *userdata);  ///< 错误事件
    void (*on_state_changed)(
        tcp_conn_item_t *conn,
        conn_state_t     old_state,
        conn_state_t     new_state,
        void            *userdata);  ///< 状态变化事件
    void *userdata;       ///< 用户数据指针
};

// TCP连接管理器操作接口 - 精细化设计
struct tcp_conn_manage_ops_s
{
    // ========== 管理器生命周期管理 ==========
    /**
     * @brief 初始化TCP连接管理器
     * @param mgr 管理器实例
     * @param settings 配置参数
     * @return 0成功，负数失败
     */
    int (*init)(tcp_conn_manage_t *mgr, const tcp_conn_mgr_settings_t *settings);

    /**
     * @brief 启动管理器（启动工作线程）
     * @param mgr 管理器实例
     * @return 0成功，负数失败
     */
    int (*start)(tcp_conn_manage_t *mgr);

    /**
     * @brief 停止管理器（停止工作线程）
     * @param mgr 管理器实例
     * @return 0成功，负数失败
     */
    int (*stop)(tcp_conn_manage_t *mgr);

    /**
     * @brief 清理管理器资源
     * @param mgr 管理器实例
     * @return 0成功，负数失败
     */
    int (*cleanup)(tcp_conn_manage_t *mgr);

    /**
     * @brief 获取管理器状态
     * @param mgr 管理器实例
     * @return 0未启动，1运行中，负数错误
     */
    int (*get_manager_state)(tcp_conn_manage_t *mgr);

    // ========== 连接建立和管理 ==========
    /**
     * @brief 创建客户端连接
     * @param mgr 管理器实例
     * @param conf 连接配置
     * @param conn 输出的连接对象指针
     * @return 0成功，负数失败
     */
    int (*create_connection)(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_item_t **conn);

    /**
     * @brief 创建服务器监听器
     * @param mgr 管理器实例
     * @param conf 监听配置
     * @param listener 输出的监听器对象指针
     * @return 0成功，负数失败
     */
    int (*create_listener)(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_listner_t **listener);

    /**
     * @brief 接受新的连接（仅用于服务器）
     * @param mgr 管理器实例
     * @param listener 监听器对象
     * @param conn 输出的新连接对象指针
     * @return 0成功，负数失败
     */
    int (*accept_connection)(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener, tcp_conn_item_t **conn);

    /**
     * @brief 销毁连接对象
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 0成功，负数失败
     */
    int (*destroy_connection)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 销毁监听器对象
     * @param mgr 管理器实例
     * @param listener 监听器对象
     * @return 0成功，负数失败
     */
    int (*destroy_listener)(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener);

    // ========== 连接控制 ==========
    /**
     * @brief 启用连接（开始连接或监听）
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 0成功，负数失败
     */
    int (*enable_connection)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 启用监听器（开始监听端口）
     * @param mgr 管理器实例
     * @param listener 监听器对象
     * @return 0成功，负数失败
     */
    int (*enable_listener)(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener);

    /**
     * @brief 禁用连接（暂停连接）
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 0成功，负数失败
     */
    int (*disable_connection)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 重新连接
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 0成功，负数失败
     */
    int (*reconnect)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 设置连接超时
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param timeout_ms 超时时间（毫秒）
     * @return 0成功，负数失败
     */
    int (*set_timeout)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int timeout_ms);

    /**
     * @brief 设置重连参数
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param max_retries 最大重试次数
     * @param interval_ms 重试间隔（毫秒）
     * @return 0成功，负数失败
     */
    int (*set_reconnect_params)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int max_retries, int interval_ms);

    // ========== 数据传输 ==========
    /**
     * @brief 发送数据
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param data 数据缓冲区
     * @param len 数据长度
     * @return 实际发送的字节数，负数表示错误
     */
    int (*send_data)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len);

    /**
     * @brief 接收数据
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param data 数据缓冲区
     * @param len 缓冲区大小
     * @return 实际接收的字节数，负数表示错误
     */
    int (*receive_data)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len);

    /**
     * @brief 发送数据（非阻塞）
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param data 数据缓冲区
     * @param len 数据长度
     * @return 实际发送的字节数，负数表示错误
     */
    int (*send_data_nonblock)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len);

    /**
     * @brief 接收数据（非阻塞）
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param data 数据缓冲区
     * @param len 缓冲区大小
     * @return 实际接收的字节数，负数表示错误
     */
    int (*receive_data_nonblock)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len);

    /**
     * @brief 获取可读数据大小
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 可读字节数，负数表示错误
     */
    int (*get_available_data)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 获取可写空间大小
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 可写字节数，负数表示错误
     */
    int (*get_available_space)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    // ========== 状态查询 ==========
    /**
     * @brief 获取连接状态
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 连接状态
     */
    conn_state_t (*get_connection_state)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 获取连接统计信息
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param stats 输出的统计信息
     * @return 0成功，负数失败
     */
    int (*get_connection_stats)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, struct tcp_conn_stats_s *stats);

    /**
     * @brief 获取本地地址信息
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param ip 输出的IP地址
     * @param port 输出的端口
     * @return 0成功，负数失败
     */
    int (*get_local_address)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port);

    /**
     * @brief 获取远程地址信息
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param ip 输出的IP地址
     * @param port 输出的端口
     * @return 0成功，负数失败
     */
    int (*get_remote_address)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port);

    /**
     * @brief 检查连接是否活跃
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 1活跃，0不活跃，负数错误
     */
    int (*is_connection_active)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    // ========== 连接关闭 ==========
    /**
     * @brief 优雅关闭连接
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 0成功，负数失败
     */
    int (*close_connection)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 强制关闭连接
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @return 0成功，负数失败
     */
    int (*force_close_connection)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

    /**
     * @brief 关闭所有连接
     * @param mgr 管理器实例
     * @return 0成功，负数失败
     */
    int (*close_all_connections)(tcp_conn_manage_t *mgr);

    // ========== 网络配置 ==========
    /**
     * @brief 绑定到指定网络接口
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param interface 网络接口名称
     * @return 0成功，负数失败
     */
    int (*bind_to_interface)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *interface);

    /**
     * @brief 设置本地地址
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param ip IP地址
     * @param port 端口号
     * @return 0成功，负数失败
     */
    int (*set_local_address)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *ip, int port);

    /**
     * @brief 设置套接字选项
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param level 选项级别
     * @param optname 选项名称
     * @param optval 选项值
     * @param optlen 选项长度
     * @return 0成功，负数失败
     */
    int (*set_socket_option)(
        tcp_conn_manage_t *mgr,
        tcp_conn_item_t   *conn,
        int                level,
        int                optname,
        const void        *optval,
        socklen_t          optlen);

    /**
     * @brief 获取套接字选项
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param level 选项级别
     * @param optname 选项名称
     * @param optval 输出的选项值
     * @param optlen 输入输出的选项长度
     * @return 0成功，负数失败
     */
    int (*get_socket_option)(
        tcp_conn_manage_t *mgr,
        tcp_conn_item_t   *conn,
        int                level,
        int                optname,
        void              *optval,
        socklen_t         *optlen);

    // ========== 事件处理 ==========
    /**
     * @brief 设置事件处理器
     * @param mgr 管理器实例
     * @param handler 事件处理器
     * @return 0成功，负数失败
     */
    int (*set_event_handler)(tcp_conn_manage_t *mgr, const tcp_conn_event_handler_t *handler);

    /**
     * @brief 处理事件（在事件循环中调用）
     * @param mgr 管理器实例
     * @param timeout_ms 超时时间（毫秒）
     * @return 处理的事件数量，负数表示错误
     */
    int (*process_events)(tcp_conn_manage_t *mgr, int timeout_ms);

    /**
     * @brief 设置事件回调（兼容性接口）
     * @param mgr 管理器实例
     * @param callback 回调函数
     * @param userdata 用户数据
     * @return 0成功，负数失败
     */
    int (*set_event_callback)(
        tcp_conn_manage_t *mgr,
        void (*callback)(tcp_conn_event_t *event, void *userdata),
        void *userdata);

    // ========== 缓冲区管理 ==========
    /**
     * @brief 设置接收缓冲区大小
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param size 缓冲区大小
     * @return 0成功，负数失败
     */
    int (*set_rx_buffer_size)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size);

    /**
     * @brief 设置发送缓冲区大小
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param size 缓冲区大小
     * @return 0成功，负数失败
     */
    int (*set_tx_buffer_size)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size);

    /**
     * @brief 获取接收缓冲区使用情况
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param used 输出的已使用字节数
     * @param total 输出的总字节数
     * @return 0成功，负数失败
     */
    int (*get_rx_buffer_info)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total);

    /**
     * @brief 获取发送缓冲区使用情况
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param used 输出的已使用字节数
     * @param total 输出的总字节数
     * @return 0成功，负数失败
     */
    int (*get_tx_buffer_info)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total);

    // ========== 高级功能 ==========
    /**
     * @brief 启用TCP_NODELAY
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param enable 是否启用
     * @return 0成功，负数失败
     */
    int (*set_tcp_nodelay)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);

    /**
     * @brief 设置保活参数
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param keepalive_time 保活时间（秒）
     * @param keepalive_intvl 保活间隔（秒）
     * @param keepalive_probes 保活探测次数
     * @return 0成功，负数失败
     */
    int (*set_keepalive)(
        tcp_conn_manage_t *mgr,
        tcp_conn_item_t   *conn,
        int                keepalive_time,
        int                keepalive_intvl,
        int                keepalive_probes);

    /**
     * @brief 启用地址重用
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param enable 是否启用
     * @return 0成功，负数失败
     */
    int (*set_reuse_addr)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);

    /**
     * @brief 启用端口重用
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param enable 是否启用
     * @return 0成功，负数失败
     */
    int (*set_reuse_port)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);

    // ========== 调试和诊断 ==========
    /**
     * @brief 获取最后错误信息
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param error_msg 输出的错误信息
     * @param max_len 错误信息缓冲区大小
     * @return 0成功，负数失败
     */
    int (*get_last_error)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *error_msg, size_t max_len);

    /**
     * @brief 转储连接信息
     * @param mgr 管理器实例
     * @param conn 连接对象
     * @param buffer 输出缓冲区
     * @param buffer_size 缓冲区大小
     * @return 实际写入的字节数，负数表示错误
     */
    int (*dump_connection_info)(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *buffer, size_t buffer_size);

    /**
     * @brief 验证连接配置
     * @param mgr 管理器实例
     * @param conf 连接配置
     * @return 0有效，负数无效
     */
    int (*validate_config)(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf);
};

struct tcp_conn_manage_s
{
    tcp_conn_mgr_settings_t      settings;
    tcp_conn_manage_type_t       type;
    const tcp_conn_manage_ops_t *ops;

    // 对应每一个配置项(但是配置可能无效, 无法建立连接)
    ts_list_t conn_cfg_list_;

    pthread_mutex_t lock;
    // 对于Server, 使用TCP端口重定向,需要额外的监听
    ts_list_t lisnter_list;

    volatile int     is_running;
    int              max_conns;
    tcp_conn_item_t *conns[MAX_CONN];
    int              conn_count;  //不区分被动还是主动

    int       rx_epoll_fd;
    int       tx_epoll_fd;
    pthread_t rx_tid;
    pthread_t tx_tid;
};

int  load_tcp_config(const char *path_to_json_file, tcp_conn_mgr_settings_t *settings);
void show_tcp_config(tcp_conn_mgr_settings_t *settings);

// 新增的配置加载函数
const tcp_conn_manage_ops_t *get_tcp_conn_ops_by_type(const char *type);
const tcp_conn_manage_ops_t *load_tcp_config_with_ops(const char *filename, tcp_conn_mgr_settings_t *settings);

// 以下辅助函数, 在 tcp_conn_misc.c 中实现
void dump_bytes_array(void *data, size_t len, size_t offset);
int  set_sock_timeout(int sockfd, int timeout_ms);
