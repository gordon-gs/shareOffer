/**
 * @file     : tcp_conn_toe.h
 * @brief    : TCP连接管理TOE实现头文件
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-25 17:41:00
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */

#pragma once

#include "tcp_conn_common.h"

#ifdef __cplusplus
extern "C" {
#endif

// TOE TCP连接管理器操作接口声明
// 管理器生命周期管理
int toe_mgr_init(tcp_conn_manage_t *mgr, const tcp_conn_mgr_settings_t *settings);
int toe_mgr_start(tcp_conn_manage_t *mgr);
int toe_mgr_stop(tcp_conn_manage_t *mgr);
int toe_mgr_cleanup(tcp_conn_manage_t *mgr);
int toe_mgr_get_manager_state(tcp_conn_manage_t *mgr);

// 连接建立和管理
int toe_mgr_create_connection(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_item_t **conn);
int toe_mgr_create_listener(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_listner_t **listener);
int toe_mgr_accept_connection(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener, tcp_conn_item_t **conn);
int toe_mgr_destroy_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_destroy_listener(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener);

// 连接控制
int toe_mgr_enable_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_disable_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_reconnect(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_set_timeout(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int timeout_ms);
int toe_mgr_set_reconnect_params(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int max_retries, int interval_ms);

// 数据传输
int toe_mgr_send_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len);
int toe_mgr_receive_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len);
int toe_mgr_send_data_nonblock(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len);
int toe_mgr_receive_data_nonblock(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len);
int toe_mgr_get_available_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_get_available_space(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

// 状态查询
conn_state_t toe_mgr_get_connection_state(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_get_connection_stats(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, struct tcp_conn_stats_s *stats);
int toe_mgr_get_local_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port);
int toe_mgr_get_remote_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port);
int toe_mgr_is_connection_active(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);

// 连接关闭
int toe_mgr_close_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_force_close_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn);
int toe_mgr_close_all_connections(tcp_conn_manage_t *mgr);

// 网络配置
int toe_mgr_bind_to_interface(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *interface);
int toe_mgr_set_local_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *ip, int port);
int toe_mgr_set_socket_option(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int level, int optname, const void *optval, socklen_t optlen);
int toe_mgr_get_socket_option(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int level, int optname, void *optval, socklen_t *optlen);

// 事件处理
int toe_mgr_set_event_handler(tcp_conn_manage_t *mgr, const tcp_conn_event_handler_t *handler);
int toe_mgr_process_events(tcp_conn_manage_t *mgr, int timeout_ms);
int toe_mgr_set_event_callback(tcp_conn_manage_t *mgr, void (*callback)(tcp_conn_event_t *event, void *userdata), void *userdata);

// 缓冲区管理
int toe_mgr_set_rx_buffer_size(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size);
int toe_mgr_set_tx_buffer_size(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size);
int toe_mgr_get_rx_buffer_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total);
int toe_mgr_get_tx_buffer_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total);

// 高级功能
int toe_mgr_set_tcp_nodelay(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);
int toe_mgr_set_keepalive(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int keepalive_time, int keepalive_intvl, int keepalive_probes);
int toe_mgr_set_reuse_addr(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);
int toe_mgr_set_reuse_port(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable);

// 调试和诊断
int toe_mgr_get_last_error(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *error_msg, size_t max_len);
int toe_mgr_dump_connection_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *buffer, size_t buffer_size);
int toe_mgr_validate_config(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf);

// TOE操作接口实例
extern const tcp_conn_manage_ops_t toe_tcp_conn_ops;

#ifdef __cplusplus
}
#endif
