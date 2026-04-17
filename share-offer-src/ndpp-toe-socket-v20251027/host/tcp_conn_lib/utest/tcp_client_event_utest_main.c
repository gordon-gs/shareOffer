/*
 * @file     : tcp_client_event_utest_main.c
 * @brief    : TCP客户端连接事件订阅测试程序
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 17:07:41
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "tcp_conn.h"
#include "tcp_conn_type.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/epoll.h>
#include <assert.h>
#include <signal.h>
#include <errno.h>
#include <time.h>

#define MAX_EVENTS   32
#define TEST_MESSAGE "Hello from TCP Client!"

tcp_conn_manage_t *g_tcp_mgr        = NULL;
tcp_conn_item_t   *g_client_conn    = NULL;
const int          g_client_conn_id = 1;
volatile int       g_running        = 1;
volatile int       g_event_count    = 0;

// 事件统计结构
typedef struct
{
    int connected_count;
    int rx_ready_count;
    int tx_ready_count;
    int closed_count;
    int error_count;
    int total_events;
} event_stats_t;

static event_stats_t g_client_stats = {0};

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
    case TCP_EVENT_CLOSED:
        return "CLOSED";
    case TCP_EVENT_ERROR:
        return "ERROR";
    default:
        return "UNKNOWN";
    }
}

// 打印事件统计信息
void print_event_stats(const char *conn_name, const event_stats_t *stats)
{
    printf("\n=== %s Event Statistics ===\n", conn_name);
    printf("RX_READY events: %d\n", stats->rx_ready_count);
    printf("TX_READY events: %d\n", stats->tx_ready_count);
    printf("CLOSED events:  %d\n", stats->closed_count);
    printf("ERROR events:   %d\n", stats->error_count);
    printf("Total events:   %d\n", stats->total_events);
    printf("===============================\n\n");
}

// 更新事件统计
void update_event_stats(event_stats_t *stats, conn_event_type_t type)
{
    stats->total_events++;
    switch (type)
    {
    case TCP_EVENT_RX_READY:
        stats->rx_ready_count++;
        break;
    case TCP_EVENT_TX_READY:
        stats->tx_ready_count++;
        break;
    case TCP_EVENT_CLOSED:
        stats->closed_count++;
        break;
    case TCP_EVENT_ERROR:
        stats->error_count++;
        break;
    default:
        break;
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

    struct epoll_event events[MAX_EVENTS];
    struct epoll_event ev;

    if (!g_client_conn)
    {
        printf("Client connection is NULL\n");
        close(epfd);
        return NULL;
    }

    int event_fd = tcp_conn_get_event_fd(g_client_conn);
    if (event_fd < 0)
    {
        printf("Failed to get event fd for client connection\n");
        close(epfd);
        return NULL;
    }

    ev.events   = EPOLLIN;
    ev.data.ptr = g_client_conn;
    if (epoll_ctl(epfd, EPOLL_CTL_ADD, event_fd, &ev) < 0)
    {
        printf("Failed to add epoll event for client connection: %s\n", strerror(errno));
        close(epfd);
        return NULL;
    }

    printf("Added event monitoring for client connection (fd: %d)\n", event_fd);
    printf("Event monitoring thread started\n");

    while (g_running)
    {
        int nfds = epoll_wait(epfd, events, MAX_EVENTS, 1000);  // 1秒超时
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
                "[Client] Event %d: Type=%s, ConnID=%d\n",
                g_event_count,
                event_type_str((conn_event_type_t)event.type),
                event.conn_id);

            update_event_stats(&g_client_stats, (conn_event_type_t)event.type);

            // 处理不同类型的事件
            switch ((conn_event_type_t)event.type)
            {
            case TCP_EVENT_RX_READY:
            {
                // 接收数据
                const void *data     = NULL;
                int         data_len = 0;
                int         result   = tcp_conn_recv(conn, &data, &data_len);
                if (result == 0 && data_len > 0)
                {
                    printf("[Client] Received %d bytes: %.*s\n", data_len, data_len, (char *)data);

                    tcp_conn_consume(conn, data_len);
                }
                break;
            }

            case TCP_EVENT_TX_READY:
            {
                printf("[Client] TX ready - can send more data\n");
                break;
            }

            case TCP_EVENT_CLOSED:
            {
                printf("[Client] Connection closed\n");
                break;
            }

            case TCP_EVENT_ERROR:
            {
                printf("[Client] Connection error occurred\n");
                break;
            }

            default:
            {
                printf("[Client] Unknown event type: %d\n", event.type);
                break;
            }
            }
        }
    }

    close(epfd);
    printf("Event monitoring thread stopped\n");
    return NULL;
}

// 数据发送线程
void *tcp_send_thread(void *arg)
{
    printf("Client send thread started\n");

    while (g_running)
    {
        sleep(5);  // 每5秒发送一次数据

        if (!g_running)
            break;

        if (g_client_conn)
        {
            int state = tcp_conn_state(g_client_conn);
            if (state == CONN_STATE_CONNECTED)
            {
                const char *test_msg = TEST_MESSAGE;
                int         result   = tcp_conn_send(g_client_conn, test_msg, strlen(test_msg));
                if (result > 0)
                {
                    printf("[Client] Sent %d bytes: %s\n", result, test_msg);
                }
                else
                {
                    printf("[Client] Failed to send: %s\n", tcp_conn_strerror(errno));
                }
            }
            else
            {
                printf("[Client] Not connected (state: %d)\n", state);
            }
        }
    }
    printf("Client send thread stopped\n");
    return NULL;
}


// 等待连接建立
int wait_for_connection(tcp_conn_item_t *conn, const char *name, int timeout_sec)
{
    time_t start_time = time(NULL);
    int    last_state = -1;

    printf("%s: Waiting for connection to establish...\n", name);

    while (g_running && (time(NULL) - start_time) < timeout_sec)
    {
        int          state_int = tcp_conn_state(conn);
        conn_state_t state     = (conn_state_t)state_int;

        // 打印状态变化
        if (state != last_state)
        {
            printf("%s: Connection state changed to %d (%s)\n", name, state, conn_state_str(state));
            last_state = state;
        }

        if (state == CONN_STATE_CONNECTED)
        {
            printf("%s: Connection established successfully\n", name);
            return 0;
        }
        else if (state == CONN_STATE_CLOSED)
        {
            printf("%s: Connection failed with state %d (%s)\n", name, state, conn_state_str(state));
            return -1;
        }

        // 每2秒打印一次等待状态
        if ((time(NULL) - start_time) % 2 == 0 && state == last_state)
        {
            printf(
                "%s: Still waiting for connection... (state: %d (%s), elapsed: %lds)\n",
                name,
                state,
                conn_state_str(state),
                time(NULL) - start_time);
        }

        usleep(100000);  // 100ms
    }

    printf(
        "%s: Connection timeout after %d seconds (final state: %d (%s))\n",
        name,
        timeout_sec,
        last_state,
        conn_state_str((conn_state_t)last_state));
    return -1;
}

int main(int argc, char *argv[])
{
    printf("=== TCP Client Event Subscription Test ===\n");

    // 设置信号处理
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    // 1. 初始化，加载通道配置
    const char *config_file = "tcp_conn_loopback_config.json";
    if (argc > 1) {
        config_file = argv[1];
    }
    printf("Loading configuration from %s...\n", config_file);
    g_tcp_mgr = tcp_conn_mgr_create(config_file);
    if (!g_tcp_mgr)
    {
        printf("Failed to create TCP manager: %s\n", tcp_conn_strerror(errno));
        return -1;
    }
    printf("TCP manager created successfully\n");

    // 2. 查找服务器和客户端连接对象
    printf("Looking up server connection...\n");
    tcp_conn_item_t *server_conn = tcp_conn_find_by_id(g_tcp_mgr, 0);  // 服务器ID为0
    if (!server_conn)
    {
        printf("Failed to find server connection (ID: 0)\n");
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    printf("Server connection found (ID: 0)\n");

    printf("Looking up client connection...\n");
    g_client_conn = tcp_conn_find_by_id(g_tcp_mgr, g_client_conn_id);
    if (!g_client_conn)
    {
        printf("Failed to find client connection (ID: %d)\n", g_client_conn_id);
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    printf("Client connection found (ID: %d)\n", g_client_conn_id);
#if 1
    // 3. 启动服务器监听
    printf("Starting server listener...\n");
    int server_result = tcp_conn_listen(server_conn);
    if (server_result != 0)
    {
        printf("Failed to start server listener, result: %d, error: %s\n", server_result, tcp_conn_strerror(errno));
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    else
    {
        printf("Server listener started successfully\n");
    }

    // 等待服务器启动并进入监听状态
    printf("Waiting for server to enter LISTENING state...\n");
    int server_state = -1;
    int server_ready = 0;
    time_t server_start_time = time(NULL);

    // 等待最多5秒让服务器进入监听状态
    while (time(NULL) - server_start_time < 5)
    {
        server_state = tcp_conn_state(server_conn);
        printf("Server state: %d (%s)\n", server_state, conn_state_str((conn_state_t)server_state));

        if (server_state == CONN_STATE_LISTENING)
        {
            printf("Server is now in LISTENING state\n");
            server_ready = 1;
            break;
        }
        else if (server_state == CONN_STATE_CLOSED)
        {
            printf("Server failed to start, state is CLOSED\n");
            break;
        }

        sleep(1);
    }

    if (!server_ready)
    {
        printf("ERROR: Server failed to enter LISTENING state (final state: %d (%s))\n",
               server_state, conn_state_str((conn_state_t)server_state));
        printf("This may indicate that the server failed to bind to the port or the port is already in use.\n");

        // 获取服务器连接信息
        const tcp_conn_info_t *server_info = tcp_conn_get_info(server_conn);
        if (server_info)
        {
            printf("Server connection info:\n");
            printf("  Local IP: %s\n", server_info->local_ip);
            printf("  Local Port: %d\n", server_info->local_port);
            printf("  Connection enabled: %d\n", server_info->conn_enabled);
        }

        // 继续测试，看看客户端是否能连接
        printf("Continuing with client connection attempt...\n");
    }
#endif
    // 4. 启动客户端连接
    printf("Starting client connection...\n");
    int client_result = tcp_conn_connect(g_client_conn);
    if (client_result != 0)
    {
        printf("Failed to start client connection, result: %d, error: %s\n", client_result, tcp_conn_strerror(errno));
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    else
    {
        printf("Client connection started successfully\n");
    }

    // 等待连接建立
    printf("Waiting for connection to establish...\n");
    int client_connected = wait_for_connection(g_client_conn, "Client", 10);

    if (!client_connected)
    {
        printf("Failed to establish connection\n");

        // 打印服务器和客户端的最终状态
        printf(
            "Final server state: %d (%s)\n",
            tcp_conn_state(server_conn),
            conn_state_str((conn_state_t)tcp_conn_state(server_conn)));
        printf(
            "Final client state: %d (%s)\n",
            tcp_conn_state(g_client_conn),
            conn_state_str((conn_state_t)tcp_conn_state(g_client_conn)));

        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }

    // 4. 启动事件监听线程
    printf("Starting event monitoring thread...\n");
    pthread_t event_thread;
    if (pthread_create(&event_thread, NULL, tcp_event_thread, NULL) != 0)
    {
        printf("Failed to create event thread: %s\n", strerror(errno));
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }

    // 5. 启动数据发送线程
    printf("Starting send thread...\n");
    pthread_t send_thread;
    if (pthread_create(&send_thread, NULL, tcp_send_thread, NULL) != 0)
    {
        printf("Failed to create send thread: %s\n", strerror(errno));
    }

    printf("Client event subscription test running... Press Ctrl+C to stop\n");
    printf("Test will run for 60 seconds or until interrupted\n");

    // 运行测试
    int count = 0;
    while (g_running && count < 60)
    {  // 运行60秒
        sleep(1);
        count++;

        // 每10秒打印统计信息
        if (count % 10 == 0)
        {
            printf("\n--- Test Progress: %d/60 seconds ---\n", count);
            printf("Total events received: %d\n", g_event_count);
            print_event_stats("Client", &g_client_stats);
        }
    }

    printf("\nStopping client test...\n");

    // 等待线程结束
    printf("Waiting for threads to finish...\n");
    pthread_join(event_thread, NULL);
    pthread_join(send_thread, NULL);

    // 打印最终统计
    printf("\n=== Final Client Event Statistics ===\n");
    printf("Total events processed: %d\n", g_event_count);
    print_event_stats("Client", &g_client_stats);

    // 清理资源
    printf("Closing client connection...\n");
    tcp_conn_close(g_client_conn);

    printf("Destroying TCP manager...\n");
    tcp_conn_mgr_destroy(g_tcp_mgr);

    printf("Client event subscription test completed\n");
    return 0;
}
