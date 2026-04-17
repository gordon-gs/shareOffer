#ifndef TCP_CONN_H
#define TCP_CONN_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct tcp_conn_manage_s tcp_conn_manage_t;
typedef struct tcp_conn_item_s   tcp_conn_item_t;
typedef struct tcp_conn_info_s   tcp_conn_info_t;
typedef struct tcp_conn_stats_s  tcp_conn_stats_t;

#include "tcp_conn_type.h"

// 版本信息字符串. eg. 0.2.0
const char *tcp_conn_lib_version(void);

#define RETURN_ERROR(errcode) \
    do                        \
    {                         \
        errno = (errcode);    \
        return -1;            \
    } while (0)
// 错误码转换为字符串
const char *tcp_conn_strerror(int err);

// 加载配置文件创建管理器(进程实例唯一, 阻塞调用)
tcp_conn_manage_t *tcp_conn_mgr_create(const char *config_path);

// 销毁(未关闭的通道将尝试安全关闭,阻塞调用)
void tcp_conn_mgr_destroy(tcp_conn_manage_t *tcp_mgr);

// 通过 conn_id 查找连接对象
tcp_conn_item_t *tcp_conn_find_by_id(tcp_conn_manage_t *tcp_mgr, uint16_t conn_id);

// 获取连接信息
const tcp_conn_info_t *tcp_conn_get_info(tcp_conn_item_t *tcp_conn);

// 获取当前连接的 pipe 读端（用于事件通知）
int tcp_conn_get_event_fd(tcp_conn_item_t *conn);

// 异步发送数据（写入 tx-buf）
int tcp_conn_send(tcp_conn_item_t *tcp_conn, const void *data, const int len);

// 异步接收数据（注意, 参数传入返回的是数据的地址，非拷贝）
int tcp_conn_recv(tcp_conn_item_t *tcp_conn, const void **ret_data, int *ret_data_len);

// 消费 rx-buf 的数据(移动缓冲区头部的标记位置)
int tcp_conn_consume(tcp_conn_item_t *tcp_conn, const int len);

// 创建 TCP 客户端连接
int tcp_conn_connect(tcp_conn_item_t *tcp_conn);

// 创建 TCP 服务端监听(等待配置连接)
int tcp_conn_listen(tcp_conn_item_t *tcp_conn);

// 安全关闭
int tcp_conn_close(tcp_conn_item_t *tcp_conn);

// 强制关闭(会丢弃未完成发送和接收的缓冲数据,立即关闭连接)
int tcp_conn_reset(tcp_conn_item_t *tcp_conn);

// 查询连接状态
int tcp_conn_state(tcp_conn_item_t *conn);

#ifdef __cplusplus
}
#endif

#endif  // TCP_CONN_H
