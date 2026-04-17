/*
 * @file     : tcp_iperf_forward_utest_main.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-24 18:12:25
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

#define MAX_EVENTS 32

tcp_conn_manage_t *g_tcp_mgr = NULL;
tcp_conn_item_t   *g_tcp_conn_list[2];
const uint16_t     g_tcp_server_conn_id = 0;
const uint16_t     g_tcp_client_conn_id = 1;

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
        int n = epoll_wait(epfd, events, MAX_EVENTS, -1);
        for (int i = 0; i < n; ++i)
        {
            tcp_conn_item_t *conn       = (tcp_conn_item_t *)events[i].data.ptr;
            int              rx_pipe_fd = tcp_conn_get_event_fd(conn);
            tcp_conn_event_t evt;
            //! 从pipe上读事件
            read(rx_pipe_fd, &evt, sizeof(evt));

            if (evt.type == TCP_EVENT_RX_READY)
            {
                const void *data;
                int         len;
                //! 获取数据咋 ringbuffer 中的位置指针(零拷贝)
                if (tcp_conn_recv(conn, &data, &len) == 0 && len > 0)
                {
                    printf("[app] ConnID %d received %zu bytes.\n", evt.conn_id, len, (int)len);

                    uint16_t src_id = evt.conn_id;
                    //! 在 0 1, 这 2 个 tcp 连接之间双向转发payload
                    int              dst_id   = (src_id == 0) ? 1 : 0;
                    tcp_conn_item_t *dst_conn = tcp_conn_find_by_id(mgr, dst_id);

                    if (dst_conn && tcp_conn_state(dst_conn) == CONN_STATE_CONNECTED)
                    {
                        printf("[app] ConnID %d try forward %zu bytes.\n", dst_id, len, (int)len);
                        //! 转发的目标通道有效,进行转发
                        tcp_conn_send(dst_conn, data, len);
                    }
                    //! 零拷贝操作,需要在取走数据后, 更新在缓冲区中的位置标记(配合 recv 接口使用)
                    tcp_conn_consume(conn, len);
                }
            }
            else if (evt.type == TCP_EVENT_CLOSED)
            {
                printf("[app] ConnID %d closed.\n", evt.conn_id);
            }
        }
    }
    return NULL;
}

int main(int argc, char *argv[])
{
    // 1. 初始化
    const char *config_file = "tcp_conn_loopback_config.json";
    if (argc > 1) {
        config_file = argv[1];
    }
    printf("Loading configuration from %s...\n", config_file);
    g_tcp_mgr = tcp_conn_mgr_create(config_file);

    // 2. 加载通道配置
    g_tcp_conn_list[0] = tcp_conn_find_by_id(g_tcp_mgr, g_tcp_server_conn_id);
    g_tcp_conn_list[1] = tcp_conn_find_by_id(g_tcp_mgr, g_tcp_client_conn_id);

    // 3. 启动连接管理器
    tcp_conn_listen(g_tcp_conn_list[0]);
    tcp_conn_connect(g_tcp_conn_list[1]);

    // 4. 启动事件线程
    pthread_t t_forward;
    pthread_create(&t_forward, NULL, tcp_forward_app_thread, g_tcp_mgr);

    pause();

    tcp_conn_close(g_tcp_conn_list[0]);
    tcp_conn_close(g_tcp_conn_list[1]);

    tcp_conn_mgr_destroy(g_tcp_mgr);

    return 0;
}
