/*
 * @file     : tcp_server_event_utest_main.c
 * @brief    : TCP服务器端连接事件订阅测试程序
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 11:15:19
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
#include <assert.h>
#include <signal.h>
#include <errno.h>
#include <time.h>

#define MAX_EVENTS 32
#define TEST_MESSAGE "Hello from TCP Server!"

tcp_conn_manage_t *g_tcp_mgr = NULL;
tcp_conn_item_t   *g_server_conn = NULL;
const int          g_server_conn_id = 0;
volatile int       g_running = 1;
volatile int       g_event_count = 0;

// 事件统计结构
typedef struct {
    int rx_ready_count;
    int tx_ready_count;
    int closed_count;
    int error_count;
    int total_events;
} event_stats_t;

static event_stats_t g_server_stats = {0};

// 信号处理函数
void signal_handler(int sig) {
    printf("\nReceived signal %d, shutting down...\n", sig);
    g_running = 0;
}

// 获取事件类型字符串
const char* event_type_str(conn_event_type_t type) {
    switch (type) {
        case TCP_EVENT_RX_READY: return "RX_READY";
        case TCP_EVENT_TX_READY: return "TX_READY";
        case TCP_EVENT_CLOSED: return "CLOSED";
        case TCP_EVENT_ERROR: return "ERROR";
        default: return "UNKNOWN";
    }
}

// 打印事件统计信息
void print_event_stats(const char* conn_name, const event_stats_t* stats) {
    printf("\n=== %s Event Statistics ===\n", conn_name);
    printf("RX_READY events: %d\n", stats->rx_ready_count);
    printf("TX_READY events: %d\n", stats->tx_ready_count);
    printf("CLOSED events:  %d\n", stats->closed_count);
    printf("ERROR events:   %d\n", stats->error_count);
    printf("Total events:   %d\n", stats->total_events);
    printf("===============================\n\n");
}

// 更新事件统计
void update_event_stats(event_stats_t* stats, conn_event_type_t type) {
    stats->total_events++;
    switch (type) {
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
void* tcp_event_thread(void* arg) {
    int epfd = epoll_create1(0);
    if (epfd < 0) {
        printf("Failed to create epoll: %s\n", strerror(errno));
        return NULL;
    }

    struct epoll_event events[MAX_EVENTS];
    struct epoll_event ev;

    if (!g_server_conn) {
        printf("Server connection is NULL\n");
        close(epfd);
        return NULL;
    }

    int event_fd = tcp_conn_get_event_fd(g_server_conn);
    if (event_fd < 0) {
        printf("Failed to get event fd for server connection\n");
        close(epfd);
        return NULL;
    }

    ev.events = EPOLLIN;
    ev.data.ptr = g_server_conn;
    if (epoll_ctl(epfd, EPOLL_CTL_ADD, event_fd, &ev) < 0) {
        printf("Failed to add epoll event for server connection: %s\n", strerror(errno));
        close(epfd);
        return NULL;
    }

    printf("Added event monitoring for server connection (fd: %d)\n", event_fd);
    printf("Event monitoring thread started\n");

    while (g_running) {
        int nfds = epoll_wait(epfd, events, MAX_EVENTS, 1000); // 1秒超时
        if (nfds < 0) {
            if (errno == EINTR) continue;
            printf("epoll_wait error: %s\n", strerror(errno));
            break;
        }

        for (int i = 0; i < nfds; i++) {
            tcp_conn_item_t* conn = (tcp_conn_item_t*)events[i].data.ptr;
            if (!conn) continue;

            int event_fd = tcp_conn_get_event_fd(conn);
            if (event_fd < 0) continue;

            // 读取事件
            tcp_conn_event_t event;
            ssize_t bytes_read = read(event_fd, &event, sizeof(event));
            if (bytes_read != sizeof(event)) {
                if (bytes_read < 0) {
                    printf("Failed to read event: %s\n", strerror(errno));
                } else {
                    printf("Incomplete event read: %zd bytes\n", bytes_read);
                }
                continue;
            }

            g_event_count++;
            printf("[Server] Event %d: Type=%s, ConnID=%d\n",
                   g_event_count, event_type_str((conn_event_type_t)event.type), event.conn_id);

            update_event_stats(&g_server_stats, (conn_event_type_t)event.type);

            // 处理不同类型的事件
            switch ((conn_event_type_t)event.type) {
                case TCP_EVENT_RX_READY: {
                    // 接收数据
                    const void* data = NULL;
                    int data_len = 0;
                    int result = tcp_conn_recv(conn, &data, &data_len);
                    if (result == 0 && data_len > 0) {
                        printf("[Server] Received %d bytes: %.*s\n",
                               data_len, data_len, (char*)data);

                        // 回显数据
                        tcp_conn_send(conn, data, data_len);
                        printf("[Server] Echoed %d bytes back\n", data_len);

                        tcp_conn_consume(conn, data_len);
                    }
                    break;
                }

                case TCP_EVENT_TX_READY: {
                    printf("[Server] TX ready - can send more data\n");
                    break;
                }

                case TCP_EVENT_CLOSED: {
                    printf("[Server] Connection closed\n");
                    break;
                }

                case TCP_EVENT_ERROR: {
                    printf("[Server] Connection error occurred\n");
                    break;
                }

                default: {
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

// 数据发送线程
void* tcp_send_thread(void* arg) {
    printf("Server send thread started\n");

    while (g_running) {
        sleep(10); // 每10秒发送一次数据

        if (!g_running) break;

        if (g_server_conn) {
            int state = tcp_conn_state(g_server_conn);
            if (state == CONN_STATE_CONNECTED) {
                const char* test_msg = TEST_MESSAGE;
                int result = tcp_conn_send(g_server_conn, test_msg, strlen(test_msg));
                if (result > 0) {
                    printf("[Server] Sent %d bytes: %s\n", result, test_msg);
                } else {
                    printf("[Server] Failed to send: %s\n", tcp_conn_strerror(errno));
                }
            } else {
                printf("[Server] Not connected (state: %d)\n", state);
            }
        }
    }

    printf("Server send thread stopped\n");
    return NULL;
}

// 等待连接建立
int wait_for_connection(tcp_conn_item_t *conn, const char *name, int timeout_sec) {
    time_t start_time = time(NULL);

    while (g_running && (time(NULL) - start_time) < timeout_sec) {
        int state_int = tcp_conn_state(conn);
        conn_state_t state = (conn_state_t)state_int;

        if (state == CONN_STATE_CONNECTED) {
            printf("%s: Connection established successfully\n", name);
            return 0;
        } else if (state == CONN_STATE_CLOSED) {
            printf("%s: Connection failed with state %d\n", name, state);
            return -1;
        }

        usleep(100000); // 100ms
    }

    printf("%s: Connection timeout after %d seconds\n", name, timeout_sec);
    return -1;
}

int main(int argc, char *argv[]) {
    printf("=== TCP Server Event Subscription Test ===\n");

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
    if (!g_tcp_mgr) {
        printf("Failed to create TCP manager: %s\n", tcp_conn_strerror(errno));
        return -1;
    }
    printf("TCP manager created successfully\n");

    // 2. 查找服务器连接对象
    printf("Looking up server connection...\n");
    g_server_conn = tcp_conn_find_by_id(g_tcp_mgr, g_server_conn_id);
    if (!g_server_conn) {
        printf("Failed to find server connection (ID: %d)\n", g_server_conn_id);
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    printf("Server connection found (ID: %d)\n", g_server_conn_id);

    // 3. 启动服务器监听
    printf("Starting server listener...\n");
    int server_result = tcp_conn_listen(g_server_conn);
    if (server_result != 0) {
        printf("Failed to start server listener, result: %d, error: %s\n",
               server_result, tcp_conn_strerror(errno));
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    } else {
        printf("Server listener started successfully\n");
    }

    // 4. 启动事件监听线程
    printf("Starting event monitoring thread...\n");
    pthread_t event_thread;
    if (pthread_create(&event_thread, NULL, tcp_event_thread, NULL) != 0) {
        printf("Failed to create event thread: %s\n", strerror(errno));
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }

    // 5. 启动数据发送线程
    printf("Starting send thread...\n");
    pthread_t send_thread;
    if (pthread_create(&send_thread, NULL, tcp_send_thread, NULL) != 0) {
        printf("Failed to create send thread: %s\n", strerror(errno));
    }

    printf("Server event subscription test running... Press Ctrl+C to stop\n");
    printf("Test will run for 120 seconds or until interrupted\n");
    printf("Waiting for client connections...\n");

    // 运行测试
    int count = 0;
    while (g_running && count < 120) {  // 运行120秒
        sleep(1);
        count++;

        // 每15秒打印统计信息
        if (count % 15 == 0) {
            printf("\n--- Test Progress: %d/120 seconds ---\n", count);
            printf("Total events received: %d\n", g_event_count);
            print_event_stats("Server", &g_server_stats);
        }
    }

    printf("\nStopping server test...\n");

    // 等待线程结束
    printf("Waiting for threads to finish...\n");
    pthread_join(event_thread, NULL);
    pthread_join(send_thread, NULL);

    // 打印最终统计
    printf("\n=== Final Server Event Statistics ===\n");
    printf("Total events processed: %d\n", g_event_count);
    print_event_stats("Server", &g_server_stats);

    // 清理资源
    printf("Closing server connection...\n");
    tcp_conn_close(g_server_conn);

    printf("Destroying TCP manager...\n");
    tcp_conn_mgr_destroy(g_tcp_mgr);

    printf("Server event subscription test completed\n");
    return 0;
}
