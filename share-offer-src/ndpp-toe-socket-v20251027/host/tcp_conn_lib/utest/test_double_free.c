#include "tcp_conn.h"
#include <stdio.h>
#include <unistd.h>

int main()
{
    printf("Testing double free fix...\n");

    // 创建TCP管理器
    tcp_conn_manage_t *mgr = tcp_conn_mgr_create("example/loopback_config.json");
    if (!mgr) {
        printf("Failed to create TCP manager\n");
        return -1;
    }

    printf("TCP manager created successfully\n");

    // 等待一小段时间让连接建立
    sleep(1);

    // 销毁TCP管理器 - 这里应该不会出现双重释放
    printf("Destroying TCP manager...\n");
    tcp_conn_mgr_destroy(mgr);

    printf("Test completed successfully - no double free detected!\n");
    return 0;
}
