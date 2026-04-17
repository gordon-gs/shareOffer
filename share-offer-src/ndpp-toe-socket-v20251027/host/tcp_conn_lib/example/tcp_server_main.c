#include "tcp_conn.h"
#include "tcp_conn_type.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <signal.h>
#include <errno.h>
#include <time.h>
#include <pthread.h>
#include <sys/epoll.h>

tcp_conn_manage_t *g_tcp_mgr     = NULL;
tcp_conn_item_t   *g_server_conn = NULL;
volatile int       g_running     = 1;
volatile int       g_event_count = 0;

// 信号处理函数
void signal_handler(int sig)
{
    printf("\nReceived signal %d, shutting down...\n", sig);
    g_running = 0;
}


// 获取事件类型字符串
const char *event_type_str(conn_event_type_t type)
{
    switch (type)
    {
    case TCP_EVENT_RX_READY:
        return "RX_READY";
    case TCP_EVENT_TX_READY:
        return "TX_READY";
    case TCP_EVENT_CONNECTED:
        return "CONNECTED";
    case TCP_EVENT_CLOSED:
        return "CLOSED";
    case TCP_EVENT_ERROR:
        return "ERROR";
    default:
        return "UNKNOWN";
    }
}

// 事件处理线程
void *tcp_event_thread(void *arg)
{
    int epfd = epoll_create1(0);
    if (epfd < 0)
    {
        printf("Failed to create epoll: %s\n", strerror(errno));
        return NULL;
    }

    struct epoll_event events[32];
    struct epoll_event ev;

    if (!g_server_conn)
    {
        printf("Server connection is NULL\n");
        close(epfd);
        return NULL;
    }

    int event_fd = tcp_conn_get_event_fd(g_server_conn);
    if (event_fd < 0)
    {
        printf("Failed to get event fd for server connection\n");
        close(epfd);
        return NULL;
    }

    ev.events   = EPOLLIN;
    ev.data.ptr = g_server_conn;
    if (epoll_ctl(epfd, EPOLL_CTL_ADD, event_fd, &ev) < 0)
    {
        printf("Failed to add epoll event for server connection: %s\n", strerror(errno));
        close(epfd);
        return NULL;
    }

    printf("Added event monitoring for server connection (fd: %d)\n", event_fd);
    printf("Event monitoring thread started\n");

    while (g_running)
    {
        int nfds = epoll_wait(epfd, events, 32, 1000);  // 1秒超时
        if (nfds < 0)
        {
            if (errno == EINTR)
                continue;
            printf("epoll_wait error: %s\n", strerror(errno));
            break;
        }

        for (int i = 0; i < nfds; i++)
        {
            tcp_conn_item_t *conn = (tcp_conn_item_t *)events[i].data.ptr;
            if (!conn)
                continue;

            int event_fd = tcp_conn_get_event_fd(conn);
            if (event_fd < 0)
                continue;

            // 读取事件
            tcp_conn_event_t event;
            ssize_t          bytes_read = read(event_fd, &event, sizeof(event));
            if (bytes_read != sizeof(event))
            {
                if (bytes_read < 0)
                {
                    printf("Failed to read event: %s\n", strerror(errno));
                }
                else
                {
                    printf("Incomplete event read: %zd bytes\n", bytes_read);
                }
                continue;
            }

            g_event_count++;
            printf(
                "[Server] Event %d: Type=%s, ConnID=%d\n",
                g_event_count,
                event_type_str((conn_event_type_t)event.type),
                event.conn_id);

            // 处理不同类型的事件
            switch ((conn_event_type_t)event.type)
            {
            case TCP_EVENT_CONNECTED:
            {
                printf("[Server] Client connected successfully\n");
                break;
            }

            case TCP_EVENT_RX_READY:
            {
                // 接收数据
                const void *data     = NULL;
                int         data_len = 0;
                int         result   = tcp_conn_recv(conn, &data, &data_len);
                if (result == 0 && data_len > 0)
                {
                    printf("[Server] Received %d bytes: %.*s\n", data_len, data_len, (char *)data);

                    // 回显数据
                    tcp_conn_send(conn, data, data_len);
                    printf("[Server] Echoed %d bytes back\n", data_len);

                    tcp_conn_consume(conn, data_len);
                }
                break;
            }

            case TCP_EVENT_TX_READY:
            {
                printf("[Server] TX ready - can send more data\n");
                break;
            }

            case TCP_EVENT_CLOSED:
            {
                printf("[Server] Connection is closed\n");
                break;
            }

            case TCP_EVENT_ERROR:
            {
                printf("[Server] Connection error occurred\n");
                break;
            }

            default:
            {
                printf("[Server] Unknown event type: %d\n", event.type);
                break;
            }
            }
        }
    }

    close(epfd);
    printf("Event monitoring thread stopped\n");
    return NULL;
}

// 简单的 echo 服务
int echo_service()
{
    printf("Starting echo service with event-driven processing...\n");
    printf("Waiting for client connections and data...\n");

    // 启动事件监听线程
    pthread_t event_thread;
    if (pthread_create(&event_thread, NULL, tcp_event_thread, NULL) != 0)
    {
        printf("Failed to create event thread: %s\n", strerror(errno));
        return -1;
    }

    // 主线程等待
    while (g_running)
    {
        // 检查服务器状态
        int server_state = tcp_conn_state(g_server_conn);
        if (server_state == CONN_STATE_CLOSED)
        {
            printf("Server connection closed, exiting...\n");
            break;
        }

        sleep(1);  // 每秒检查一次状态
    }

    // 等待事件线程结束
    printf("Waiting for event thread to finish...\n");
    pthread_join(event_thread, NULL);

    printf("Echo service stopped\n");
    return 0;
}

int main(int argc, char *argv[])
{
    printf("=== TCP Server Echo Service ===\n");

    // 设置信号处理
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    // 1. 初始化，加载通道配置
    const char *config_file = "loopback_config.json";
    if (argc > 1)
    {
        config_file = argv[1];
    }

    g_tcp_mgr = tcp_conn_mgr_create(config_file);

    // 2. 查找服务器连接对象 (ID=0) - 这应该是accept worker连接
    g_server_conn = tcp_conn_find_by_id(g_tcp_mgr, 0);

    // 3. 启用服务器监听
    tcp_conn_listen(g_server_conn);

    printf("Server listener enabled successfully\n");
    printf(
        "Accept worker state: %d (%s)\n",
        tcp_conn_state(g_server_conn),
        conn_state_str((conn_state_t)tcp_conn_state(g_server_conn)));
    printf("Press Ctrl+C to stop the server\n");

    // 4. 启用服务器监听
    echo_service();

    // 5. 清理资源
    printf("Closing server connection...\n");
    tcp_conn_close(g_server_conn);

    // 6. 清理资源
    printf("Destroying TCP manager...\n");
    tcp_conn_mgr_destroy(g_tcp_mgr);

    return 0;
}
