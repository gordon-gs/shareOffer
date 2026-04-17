#include "tcp_conn.h"
#include "tcp_conn_type.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <signal.h>
#include <errno.h>
#include <time.h>

#define TEST_MESSAGE_SIZE 1024
#define TEST_COUNT 10

tcp_conn_manage_t *g_tcp_mgr = NULL;
tcp_conn_item_t   *g_client_conn = NULL;
volatile int       g_running = 1;

// 信号处理函数
void signal_handler(int sig)
{
    printf("\nReceived signal %d, shutting down...\n", sig);
    g_running = 0;
}


// 等待连接建立
int wait_for_connection(tcp_conn_item_t *conn, const char *name, int timeout_sec)
{
    time_t start_time = time(NULL);

    printf("%s: Waiting for connection to establish...\n", name);

    while (g_running && (time(NULL) - start_time) < timeout_sec)
    {
        int state_int = tcp_conn_state(conn);
        conn_state_t state = (conn_state_t)state_int;

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

        usleep(100000);  // 100ms
    }

    printf("%s: Connection timeout after %d seconds\n", name, timeout_sec);
    return -1;
}

// 简单的 ping-pong 测试
int ping_pong_test()
{
    char send_buffer[TEST_MESSAGE_SIZE];
    char recv_buffer[TEST_MESSAGE_SIZE];

    // 准备测试数据
    memset(send_buffer, 'A', TEST_MESSAGE_SIZE - 1);
    send_buffer[TEST_MESSAGE_SIZE - 1] = '\0';

    printf("Starting ping-pong test with %d messages, each %d bytes...\n", TEST_COUNT, TEST_MESSAGE_SIZE);

    for (int i = 0; i < TEST_COUNT && g_running; i++)
    {
        // 发送数据
        printf("Test %d: Sending %d bytes...\n", i + 1, TEST_MESSAGE_SIZE);
        int send_result = tcp_conn_send(g_client_conn, send_buffer, TEST_MESSAGE_SIZE);
        if (send_result < 0)
        {
            printf("Failed to send data: %s\n", tcp_conn_strerror(errno));
            return -1;
        }
        printf("Sent %d bytes successfully\n", send_result);

        // 等待并接收回复
        usleep(100000);  // 等待100ms让服务器处理

        const void *recv_data = NULL;
        int recv_len = 0;
        int recv_result = tcp_conn_recv(g_client_conn, &recv_data, &recv_len);

        if (recv_result == 0 && recv_len > 0)
        {
            printf("Received %d bytes in response\n", recv_len);
            tcp_conn_consume(g_client_conn, recv_len);
        }
        else
        {
            printf("No data received or error: %s\n", tcp_conn_strerror(errno));
        }

        sleep(1);  // 间隔1秒
    }

    printf("Ping-pong test completed\n");
    return 0;
}

int main(int argc, char *argv[])
{
    printf("=== TCP Client Ping-Pong Test ===\n");

    // 设置信号处理
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    // 1. 初始化，加载通道配置
    const char *config_file = "loopback_config.json";
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

    // 2. 查找客户端连接对象 (ID=0)
    printf("Looking up client connection (ID=0)...\n");
    g_client_conn = tcp_conn_find_by_id(g_tcp_mgr, 0);
    if (!g_client_conn)
    {
        printf("Failed to find client connection (ID: 1)\n");
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    printf("Client connection found (ID: 1)\n");

    // 3. 启用客户端连接
    printf("Enabling client connection...\n");
    int client_result = tcp_conn_connect(g_client_conn);
    if (client_result != 0)
    {
        printf("Failed to enable client connection, result: %d, error: %s\n", client_result, tcp_conn_strerror(errno));
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }
    else
    {
        printf("Client connection enabled successfully\n");
    }

    // 4. 等待连接建立
    printf("Waiting for connection to establish...\n");
    int client_connected = wait_for_connection(g_client_conn, "Client", 10);

    if (client_connected != 0)
    {
        printf("Failed to establish connection\n");
        printf("Final client state: %d (%s)\n", tcp_conn_state(g_client_conn), conn_state_str((conn_state_t)tcp_conn_state(g_client_conn)));
        tcp_conn_mgr_destroy(g_tcp_mgr);
        return -1;
    }

    // 5. 执行 ping-pong 测试
    printf("Connection established, starting ping-pong test...\n");
    int test_result = ping_pong_test();

    // 6. 清理资源
    printf("Closing client connection...\n");
    tcp_conn_close(g_client_conn);

    printf("Destroying TCP manager...\n");
    tcp_conn_mgr_destroy(g_tcp_mgr);

    if (test_result == 0)
    {
        printf("Ping-pong test completed successfully\n");
    }
    else
    {
        printf("Ping-pong test failed\n");
    }

    return test_result;
}
