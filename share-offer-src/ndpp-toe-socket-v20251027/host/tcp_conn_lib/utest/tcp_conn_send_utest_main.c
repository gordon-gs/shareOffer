/*
 * @file     : tcp_conn_send_utest_main.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-25 18:40:28
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
#define MAX_RETRY_COUNT 5
#define RETRY_INTERVAL_MS 3000

tcp_conn_manage_t *g_tcp_mgr = NULL;
tcp_conn_item_t   *g_tcp_conn_list[2];
const int          g_tcp_server_conn_id = 0;
const int          g_tcp_client_conn_id = 1;
volatile int       g_running = 1;

// 信号处理函数
void signal_handler(int sig) {
    printf("\nReceived signal %d, shutting down...\n", sig);
    g_running = 0;
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

// 尝试重新连接
int try_reconnect(tcp_conn_item_t *conn, const char *name, int max_retries) {
    for (int i = 0; i < max_retries && g_running; i++) {
        printf("%s: Attempting reconnection %d/%d\n", name, i + 1, max_retries);

        // 关闭当前连接
        tcp_conn_close(conn);
        usleep(500000); // 500ms

        // 尝试重新连接
        int result = tcp_conn_connect(conn);
        if (result == 0) {
            printf("%s: Reconnection initiated successfully\n", name);

            // 等待连接建立
            if (wait_for_connection(conn, name, 5) == 0) {
                printf("%s: Reconnection successful\n", name);
                return 0;
            }
        } else {
            printf("%s: Reconnection failed with error: %s\n", name, tcp_conn_strerror(errno));
        }

        if (i < max_retries - 1) {
            printf("%s: Waiting %d ms before next retry...\n", name, RETRY_INTERVAL_MS);
            usleep(RETRY_INTERVAL_MS * 1000);
        }
    }

    printf("%s: All reconnection attempts failed\n", name);
    return -1;
}

void print_tcp_conn_info(const tcp_conn_info_t *info);
void print_tcp_conn_stats(const tcp_conn_info_t *info);

void safe_send(tcp_conn_item_t *tcp_conn, const void *data, const int len)
{
    const int s = tcp_conn_state(tcp_conn);
    if (CONN_STATE_CONNECTED != s)
    {
        const tcp_conn_info_t *tcp_info = tcp_conn_get_info(tcp_conn);
        assert(tcp_info);
        print_tcp_conn_info(tcp_info);
        printf(
            "tcp conn[%d] error: <%s:%d -> %s:%d > state(%d) err(%d).\n",
            tcp_info->conn_id,
            tcp_info->local_ip,
            tcp_info->local_port,
            tcp_info->remote_ip,
            tcp_info->remote_port,
            s,
            errno);

        const tcp_conn_stats_t *tcp_stats = tcp_info->stats;
        if (!tcp_stats) {
            printf("Warning: tcp_stats is null\n");
        }

        return;
    }
    const int rc = tcp_conn_send(tcp_conn, data, len);
    if (rc > 0 && rc == len)
    {
        printf("tcp conn write data done.\n");
    }
    else
    {
        if (rc == -1)
        {
            printf("tcp conn write failed with rc(%d), err(%d):%s\n", rc, errno, tcp_conn_strerror(errno));
        }
    }
}
void *tcp_ping_app_thread(void *arg)
{
    while (1)
    {
        const char *tmp_str = "1234567890...";
        {
            tcp_conn_item_t *server_conn = g_tcp_conn_list[g_tcp_server_conn_id];
            safe_send(server_conn, tmp_str, strlen(tmp_str));
        }
        {
            tcp_conn_item_t *client_conn = g_tcp_conn_list[g_tcp_client_conn_id];
            safe_send(client_conn, tmp_str, strlen(tmp_str));
        }
        sleep(3);
    }
    return NULL;
}

/* 打印函数实现 */
void print_tcp_conn_info(const tcp_conn_info_t *info)
{
    if (!info)
    {
        printf("tcp_conn_info_t: (null)\n");
        return;
    }

    printf("========== TCP Connection Info ==========\n");
    printf("Conn ID        : %d\n", info->conn_id);
    printf("Conn Name      : %s\n", info->conn_tag ? info->conn_tag : "(null)");
    printf("Conn Type      : %s (%d)\n", conn_type_str(info->conn_type), info->conn_type);

    /* 打印配置信息 */
    if (info->conn_type == CONN_TYPE_SERVER)
    {
        printf("--- Server Config ---\n");
        printf("TCP Server IP       : %s\n", info->local_ip);
        printf("TCP Server Port     : %d\n", info->local_port);
        printf("TCP Client IP       : %s\n", info->remote_ip);
        printf("TCP Client Port     : %d\n", info->remote_port);
    }
    else if (info->conn_type == CONN_TYPE_CLIENT)
    {
        printf("--- Client Config ---\n");
        printf("TCP Client IP       : %s\n", info->local_ip);
        printf("TCP Client Port     : %d\n", info->local_port);
        printf("TCP Server IP       : %s\n", info->remote_ip);
        printf("TCP Server Port     : %d\n", info->remote_port);
    }
    else
    {
        printf("--- Unknown Config ---\n");
    }

    /* 套接字信息 */
    printf("--- Socket Info ---\n");
    printf("Socket FD       : %d\n", info->rx_pipe_fd);

    /* 缓冲区信息 */
    printf("--- Buffer Info ---\n");
    printf("RX Buffer Used  : %d / %d\n", info->rx_buffer_used, info->rx_buffer_size);
    printf("TX Buffer Used  : %d / %d\n", info->tx_buffer_used, info->tx_buffer_size);

    printf("=========================================\n\n");
}

void print_tcp_conn_stats(const tcp_conn_info_t *info) {}

int main(int argc, char *argv[])
{
    printf("=== TCP Connection Loopback Test ===\n");

    // 设置信号处理
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    // 1. 初始化,加载通道配置
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

    // 2. 查找连接对象
    printf("Looking up connections...\n");
    g_tcp_conn_list[0] = tcp_conn_find_by_id(g_tcp_mgr, g_tcp_server_conn_id);
    if (!g_tcp_conn_list[0]) {
        printf("Failed to find server connection (ID: %d)\n", g_tcp_server_conn_id);
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    printf("Server connection found (ID: %d)\n", g_tcp_server_conn_id);
    print_tcp_conn_info(tcp_conn_get_info(g_tcp_conn_list[0]));

    g_tcp_conn_list[1] = tcp_conn_find_by_id(g_tcp_mgr, g_tcp_client_conn_id);
    if (!g_tcp_conn_list[1]) {
        printf("Failed to find client connection (ID: %d)\n", g_tcp_client_conn_id);
        printf("Available connections:\n");
        // 打印所有可用连接的ID和名称
        for (int i = 0; i < 10; i++) {
            tcp_conn_item_t *conn = tcp_conn_find_by_id(g_tcp_mgr, i);
            if (conn) {
                const tcp_conn_info_t *info = tcp_conn_get_info(conn);
                if (info) {
                    printf("  ID %d: %s (Type: %s)\n", i,
                           info->conn_tag ? info->conn_tag : "Unknown",
                           conn_type_str(info->conn_type));
                }
            }
        }
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    printf("Client connection found (ID: %d)\n", g_tcp_client_conn_id);
    print_tcp_conn_info(tcp_conn_get_info(g_tcp_conn_list[1]));

    // 3. 启动连接
    printf("Starting server listener...\n");
    int server_result = tcp_conn_listen(g_tcp_conn_list[0]);
    if (server_result != 0) {
        printf("Failed to start server listener, result: %d, error: %s\n",
               server_result, tcp_conn_strerror(errno));
    } else {
        printf("Server listener started successfully\n");
    }

    // 等待服务器启动
    sleep(1);

    printf("Starting client connection...\n");
    int client_result = tcp_conn_connect(g_tcp_conn_list[1]);
    if (client_result != 0) {
        printf("Failed to start client connection, result: %d, error: %s\n",
               client_result, tcp_conn_strerror(errno));
    } else {
        printf("Client connection started successfully\n");
    }

    // 等待连接建立
    printf("Waiting for connections to establish...\n");
    int server_connected = wait_for_connection(g_tcp_conn_list[0], "Server", 10);
    int client_connected = wait_for_connection(g_tcp_conn_list[1], "Client", 10);

    // 如果客户端连接失败，尝试重连
    if (!client_connected) {
        printf("Client connection failed, attempting reconnection...\n");
        if (try_reconnect(g_tcp_conn_list[1], "Client", 3) < 0) {
            printf("Failed to establish client connection after retries\n");
        } else {
            client_connected = 1;
        }
    }

    // 检查连接状态
    printf("Connection states:\n");
    printf("Server state: %d (%s)\n", tcp_conn_state(g_tcp_conn_list[0]),
           server_connected ? "Connected" : "Failed");
    printf("Client state: %d (%s)\n", tcp_conn_state(g_tcp_conn_list[1]),
           client_connected ? "Connected" : "Failed");

    // 4. 启动事件线程
    printf("Starting ping thread...\n");
    pthread_t t_ping;
    if (pthread_create(&t_ping, NULL, tcp_ping_app_thread, g_tcp_mgr) != 0) {
        printf("Failed to create ping thread: %s\n", strerror(errno));
    } else {
        printf("Ping thread started successfully\n");
    }

    printf("Test running... Press Ctrl+C to stop\n");

    // 运行测试
    int count = 0;
    while (g_running && count < 30) {  // 运行30秒后自动退出
        sleep(1);
        count++;

        // 每10秒检查连接状态并尝试重连
        if (count % 10 == 0) {
            printf("Test running... (%d/30 seconds)\n", count);

            int server_state_int = tcp_conn_state(g_tcp_conn_list[0]);
            int client_state_int = tcp_conn_state(g_tcp_conn_list[1]);
            conn_state_t server_state = (conn_state_t)server_state_int;
            conn_state_t client_state = (conn_state_t)client_state_int;

            printf("Server state: %d, Client state: %d\n", server_state, client_state);

            // 如果连接断开，尝试重连
            if (server_state != CONN_STATE_CONNECTED && server_state != CONN_STATE_LISTENING) {
                printf("Server connection lost, attempting reconnection...\n");
                tcp_conn_listen(g_tcp_conn_list[0]);
            }

            if (client_state != CONN_STATE_CONNECTED) {
                printf("Client connection lost, attempting reconnection...\n");
                try_reconnect(g_tcp_conn_list[1], "Client", 2);
            }
        }
    }

    printf("Stopping test...\n");

    // 清理资源
    printf("Closing connections...\n");
    tcp_conn_close(g_tcp_conn_list[0]);
    tcp_conn_close(g_tcp_conn_list[1]);

    printf("Destroying TCP manager...\n");
    tcp_conn_mgr_destroy(g_tcp_mgr);

    printf("Test completed\n");
    return 0;
}
