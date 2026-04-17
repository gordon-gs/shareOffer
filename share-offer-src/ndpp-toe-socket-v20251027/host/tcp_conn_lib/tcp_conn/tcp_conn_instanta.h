/**
 * @file     : tcp_conn_instanta.h
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 16:41:19
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#pragma once

// 管理器生命周期管理
int sock_mgr_init(tcp_conn_manage_t *mgr, const tcp_conn_mgr_settings_t *settings);
int sock_mgr_start(tcp_conn_manage_t *mgr);
int sock_mgr_stop(tcp_conn_manage_t *mgr);
int sock_mgr_cleanup(tcp_conn_manage_t *mgr);
int sock_mgr_get_manager_state(tcp_conn_manage_t *mgr);

// 连接建立和管理
int sock_mgr_create_connection(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_item_t **conn);
int sock_mgr_create_listener(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_listner_t **listener);
int sock_mgr_accept_connection(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener, tcp_conn_item_t **conn);
int sock_mgr_destroy_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_destroy_listener(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener);

// 连接控制
int sock_mgr_enable_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_enable_listener(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener);
int sock_mgr_disable_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_reconnect(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_set_timeout(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int timeout_ms);
int sock_mgr_set_reconnect_params(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int max_retries, int interval_ms);

// 数据传输
int sock_mgr_send_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len);
int sock_mgr_receive_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len);
int sock_mgr_send_data_nonblock(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len);
int sock_mgr_receive_data_nonblock(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len);
int sock_mgr_get_available_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_get_available_space(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

// 状态查询
conn_state_t sock_mgr_get_connection_state(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_get_connection_stats(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, struct tcp_conn_stats_s *stats);
int sock_mgr_get_local_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port);
int sock_mgr_get_remote_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port);
int sock_mgr_is_connection_active(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

// 连接关闭
int sock_mgr_close_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_force_close_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int sock_mgr_close_all_connections(tcp_conn_manage_t *mgr);

// 配置连接参数(高级选项)
int sock_mgr_bind_to_interface(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *interface);
int sock_mgr_set_local_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *ip, int port);
int sock_mgr_set_socket_option(
    tcp_conn_manage_t *mgr,
    tcp_conn_item_t   *conn,
    int                level,
    int                optname,
    const void        *optval,
    socklen_t          optlen);
int sock_mgr_get_socket_option(
    tcp_conn_manage_t *mgr,
    tcp_conn_item_t   *conn,
    int                level,
    int                optname,
    void              *optval,
    socklen_t         *optlen);

// 事件处理
int sock_mgr_set_event_handler(tcp_conn_manage_t *mgr, const tcp_conn_event_handler_t *handler);
int sock_mgr_process_events(tcp_conn_manage_t *mgr, int timeout_ms);
int sock_mgr_set_event_callback(
    tcp_conn_manage_t *mgr,
    void (*callback)(tcp_conn_event_t *event, void *userdata),
    void *userdata);

// 缓冲区管理
int sock_mgr_set_rx_buffer_size(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size);
int sock_mgr_set_tx_buffer_size(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size);
int sock_mgr_get_rx_buffer_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total);
int sock_mgr_get_tx_buffer_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total);

// 高级功能
int sock_mgr_set_tcp_nodelay(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);
int sock_mgr_set_keepalive(
    tcp_conn_manage_t *mgr,
    tcp_conn_item_t   *conn,
    int                keepalive_time,
    int                keepalive_intvl,
    int                keepalive_probes);
int sock_mgr_set_reuse_addr(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);
int sock_mgr_set_reuse_port(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);

// 调试和诊断
int sock_mgr_get_last_error(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *error_msg, size_t max_len);
int sock_mgr_dump_connection_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *buffer, size_t buffer_size);
int sock_mgr_validate_config(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf);

typedef struct socket_tcp_mgr_private_s
{
    int   epoll_fd;           ///< epoll文件描述符
    int   max_events;         ///< 最大事件数
    void *event_callback;     ///< 事件回调函数
    void *callback_userdata;  ///< 回调用户数据
} socket_tcp_mgr_private_t;

// Socket实现的操作接口
static const tcp_conn_manage_ops_t socket_conn_ops = {
    // ========== 管理器生命周期管理 ==========
    .init              = sock_mgr_init,
    .start             = sock_mgr_start,
    .stop              = sock_mgr_stop,
    .cleanup           = sock_mgr_cleanup,
    .get_manager_state = sock_mgr_get_manager_state,

    // ========== 连接建立和管理 ==========
    .create_connection  = sock_mgr_create_connection,
    .create_listener    = sock_mgr_create_listener,
    .accept_connection  = sock_mgr_accept_connection,
    .destroy_connection = sock_mgr_destroy_connection,
    .destroy_listener   = sock_mgr_destroy_listener,

    // ========== 连接控制 ==========
    .enable_connection    = sock_mgr_enable_connection,
    .enable_listener      = sock_mgr_enable_listener,
    .disable_connection   = sock_mgr_disable_connection,
    .reconnect            = sock_mgr_reconnect,
    .set_timeout          = sock_mgr_set_timeout,
    .set_reconnect_params = sock_mgr_set_reconnect_params,

    // ========== 数据传输 ==========
    .send_data             = sock_mgr_send_data,
    .receive_data          = sock_mgr_receive_data,
    .send_data_nonblock    = sock_mgr_send_data_nonblock,
    .receive_data_nonblock = sock_mgr_receive_data_nonblock,
    .get_available_data    = sock_mgr_get_available_data,
    .get_available_space   = sock_mgr_get_available_space,

    // ========== 状态查询 ==========
    .get_connection_state = sock_mgr_get_connection_state,
    .get_connection_stats = sock_mgr_get_connection_stats,
    .get_local_address    = sock_mgr_get_local_address,
    .get_remote_address   = sock_mgr_get_remote_address,
    .is_connection_active = sock_mgr_is_connection_active,

    // ========== 连接关闭 ==========
    .close_connection       = sock_mgr_close_connection,
    .force_close_connection = sock_mgr_force_close_connection,
    .close_all_connections  = sock_mgr_close_all_connections,

    // ========== 网络配置 ==========
    .bind_to_interface = sock_mgr_bind_to_interface,
    .set_local_address = sock_mgr_set_local_address,
    .set_socket_option = sock_mgr_set_socket_option,
    .get_socket_option = sock_mgr_get_socket_option,

    // ========== 事件处理 ==========
    .set_event_handler  = sock_mgr_set_event_handler,
    .process_events     = sock_mgr_process_events,
    .set_event_callback = sock_mgr_set_event_callback,

    // ========== 缓冲区管理 ==========
    .set_rx_buffer_size = sock_mgr_set_rx_buffer_size,
    .set_tx_buffer_size = sock_mgr_set_tx_buffer_size,
    .get_rx_buffer_info = sock_mgr_get_rx_buffer_info,
    .get_tx_buffer_info = sock_mgr_get_tx_buffer_info,

    // ========== 高级功能 ==========
    .set_tcp_nodelay = sock_mgr_set_tcp_nodelay,
    .set_keepalive   = sock_mgr_set_keepalive,
    .set_reuse_addr  = sock_mgr_set_reuse_addr,
    .set_reuse_port  = sock_mgr_set_reuse_port,

    // ========== 调试和诊断 ==========
    .get_last_error       = sock_mgr_get_last_error,
    .dump_connection_info = sock_mgr_dump_connection_info,
    .validate_config      = sock_mgr_validate_config,
};

int sock_mgr_start_worker_threads(tcp_conn_manage_t *mgr);
