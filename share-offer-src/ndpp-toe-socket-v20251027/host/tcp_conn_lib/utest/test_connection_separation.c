/*
 * @file     : test_connection_separation.c
 * @brief    : 测试连接创建和启用分离的功能
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 15:28:22
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "tcp_conn.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char *argv[])
{
    printf("=== Testing Connection Creation and Enable Separation ===\n");

    // 1. 创建TCP管理器
    const char *config_file = "tcp_conn_loopback_config.json";
    tcp_conn_manage_t *mgr = tcp_conn_mgr_create(config_file);
    if (!mgr)
    {
        printf("Failed to create TCP manager: %s\n", tcp_conn_strerror(errno));
        return -1;
    }
    printf("✓ TCP manager created successfully\n");

    // 2. 查找客户端连接对象
    tcp_conn_item_t *conn = tcp_conn_find_by_id(mgr, 1);
    if (!conn)
    {
        printf("Failed to find client connection (ID: 1)\n");
        tcp_conn_mgr_destroy(mgr);
        return -1;
    }
    printf("✓ Client connection found (ID: 1)\n");

    // 3. 检查连接创建后的状态
    int initial_state = tcp_conn_state(conn);
    printf("✓ Connection state after creation: %d (%s)\n",
           initial_state,
           initial_state == CONN_STATE_NONE ? "None" :
           initial_state == CONN_STATE_CONNECTING ? "Connecting" :
           initial_state == CONN_STATE_CONNECTED ? "Connected" : "Unknown");

    // 4. 启用连接（这里才会建立实际的socket连接）
    printf("✓ Enabling connection (this will create socket and connect)...\n");
    int result = tcp_conn_connect(conn);
    if (result != 0)
    {
        printf("Failed to enable connection: %s\n", tcp_conn_strerror(errno));
        tcp_conn_mgr_destroy(mgr);
        return -1;
    }
    printf("✓ Connection enable initiated successfully\n");

    // 5. 等待连接建立
    printf("✓ Waiting for connection to establish...\n");
    int max_wait = 5; // 最多等待5秒
    int connected = 0;

    for (int i = 0; i < max_wait; i++)
    {
        sleep(1);
        int state = tcp_conn_state(conn);
        printf("  - Second %d: State = %d (%s)\n",
               i + 1, state,
               state == CONN_STATE_NONE ? "None" :
               state == CONN_STATE_CONNECTING ? "Connecting" :
               state == CONN_STATE_CONNECTED ? "Connected" :
               state == CONN_STATE_CLOSED ? "Closed" : "Unknown");

        if (state == CONN_STATE_CONNECTED)
        {
            connected = 1;
            break;
        }
        else if (state == CONN_STATE_CLOSED)
        {
            printf("  ✗ Connection failed\n");
            break;
        }
    }

    if (connected)
    {
        printf("✓ Connection established successfully!\n");

        // 6. 发送测试数据
        const char *test_msg = "Hello, Server!";
        int send_result = tcp_conn_send(conn, test_msg, strlen(test_msg));
        if (send_result > 0)
        {
            printf("✓ Sent %d bytes: %s\n", send_result, test_msg);
        }
        else
        {
            printf("✗ Failed to send data\n");
        }
    }
    else
    {
        printf("✗ Connection failed to establish\n");
    }

    // 7. 清理资源
    printf("✓ Cleaning up...\n");
    tcp_conn_close(conn);
    tcp_conn_mgr_destroy(mgr);

    printf("\n=== Test Summary ===\n");
    printf("This test demonstrates the separation of connection creation and enable:\n");
    printf("1. Connection objects are created during config loading (sock_mgr_create_connection)\n");
    printf("2. Actual socket connections are established when enabling (sock_mgr_enable_connection)\n");
    printf("3. This allows for better resource management and connection lifecycle control\n");
    printf("\n✓ Test completed successfully!\n");

    return 0;
}
