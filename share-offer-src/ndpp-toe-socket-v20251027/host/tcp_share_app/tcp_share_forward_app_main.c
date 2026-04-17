/*
 * @file     : tcp_share_forward_app_main.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 22:51:53
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "tcp_conn.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/epoll.h>
#include <time.h>

#define MAX_EVENTS 32

tcp_conn_manage_t *g_tcp_mgr = NULL;
tcp_conn_item_t   *g_tcp_conn_list[2];
const int          g_tcp_server_conn_id = 0;
const int          g_tcp_client_conn_id = 1;

void *tcp_forward_app_thread(void *arg)
{
    tcp_conn_manage_t *mgr  = (tcp_conn_manage_t *)arg;
    int                epfd = epoll_create1(0);
    struct epoll_event ev;

    //! 监听pipe上的通知,是否有数据到达( tcp连接可能还没有建立, 不影响等待通知 )
    for (int i = 0; i < 2; ++i)
    {
        tcp_conn_item_t *conn = g_tcp_conn_list[i];
        if (!conn)
            continue;
        int rx_pipe_fd = tcp_conn_get_event_fd(conn);
        if (rx_pipe_fd < 0)
            continue;
        ev.data.ptr = conn;
        ev.events   = EPOLLIN;
        epoll_ctl(epfd, EPOLL_CTL_ADD, rx_pipe_fd, &ev);
    }

    struct epoll_event events[MAX_EVENTS];

    while (1)
    {
        //! 1.使用 epoll 等待 pipe 上的事件
        int n = epoll_wait(epfd, events, MAX_EVENTS, -1);
        for (int i = 0; i < n; ++i)
        {
            tcp_conn_item_t *conn       = (tcp_conn_item_t *)events[i].data.ptr;
            int              rx_pipe_fd = tcp_conn_get_event_fd(conn);
            tcp_conn_event_t evt;
            //! 2.处理 pipe 事件
            read(rx_pipe_fd, &evt, sizeof(evt));

            if (evt.type == TCP_EVENT_RX_READY)
            {
                const void *data;
                int         len;
                //! 3.读数据事件从获取数据咋 ringbuffer 中的位置指针开始(零拷贝)
                if (tcp_conn_recv(conn, &data, &len) == 0 && len > 0)
                {
                    printf("[app] ConnID %d received %zu bytes.\n", evt.conn_id, len, (int)len);

                    // 打印前8个字节的二进制内容
                    int print_len = (len > 8) ? 8 : len;
                    printf("[app] First %d bytes (binary): ", print_len);
                    const unsigned char *bytes = (const unsigned char *)data;
                    for (int j = 0; j < print_len; j++) {
                        printf("%02X ", bytes[j]);
                    }
                    printf("\n");

                    int src_id = evt.conn_id;
                    //! 4.业务逻辑:转发, 在 0 1, 这 2 个 tcp 连接之间双向转发payload
                    int              dst_id   = (src_id == 0) ? 1 : 0;
                    tcp_conn_item_t *dst_conn = tcp_conn_find_by_id(mgr, dst_id);

                    if (dst_conn && tcp_conn_state(dst_conn) == CONN_STATE_CONNECTED)
                    {
                        printf("[app] ConnID %d try forward %zu bytes.\n", dst_id, len, (int)len);
                        //! 5. 转发的目标通道有效,进行转发
                        tcp_conn_send(dst_conn, data, len);
                    }
                    //! 6. 操作接口, 需要在完成数据后, 更新在缓冲区中的位置标记(配合 tcp_conn_recv 接口使用)
                    tcp_conn_consume(conn, len);
                }
            }
            else if (evt.type == TCP_EVENT_CLOSED)
            {
                printf("[app] ConnID %d event(%d): %s.\n", evt.conn_id, evt.type, conn_event_type_str(evt.type));
            }
        }
    }
    return NULL;
}

int main(int argc, char *argv[])
{
    const char *config_file = "tcp_share_config.json";
    if (argc > 1)
    {
        config_file = argv[1];
    }
    printf("Loading configuration from %s...\n", config_file);
    // 1. 初始化,加载通道配置
    g_tcp_mgr = tcp_conn_mgr_create(config_file);

    // 2.遍历连接通道
    for (int i = 0; i < 2; i++)
    {
        g_tcp_conn_list[i]          = tcp_conn_find_by_id(g_tcp_mgr, i);
        const tcp_conn_info_t *info = tcp_conn_get_info(g_tcp_conn_list[i]);
        if (info) {
            printf("[app] Connection %d info:\n", i);
            printf("  conn_id: %d\n", info->conn_id);
            printf("  conn_tag: %s\n", info->conn_tag);
            printf("  conn_type: %s\n", conn_type_str(info->conn_type));
            printf("  local_ip: %s\n", info->local_ip);
            printf("  local_port: %d\n", info->local_port);
            printf("  remote_ip: %s\n", info->remote_ip);
            printf("  remote_port: %d\n", info->remote_port);
            printf("\n");
        }
    }

    // 3. 启动应用线程
    pthread_t t_forward;
    pthread_create(&t_forward, NULL, tcp_forward_app_thread, g_tcp_mgr);

    usleep(200 * 1000);  // wait for tcp_forward_app_thread ready

    // 4. 通过连接管理器,建立连接( 应用线程将获得事件通知 )
    tcp_conn_listen(g_tcp_conn_list[0]);
    tcp_conn_connect(g_tcp_conn_list[1]);

    // 5. 挂起主线程,任意按键将退出应用
    pause();

    // 6. 关闭连接
    tcp_conn_close(g_tcp_conn_list[0]);
    tcp_conn_close(g_tcp_conn_list[1]);

    // 7. 资源释放
    tcp_conn_mgr_destroy(g_tcp_mgr);

    return 0;
}
