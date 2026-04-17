/**
 * @file     : config_loader_test.c
 * @brief    : TCP连接配置文件加载测试工具
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 11:27:00
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// 包含tcp_conn库的头文件
#include "../tcp_conn/tcp_conn_common.h"
#include "../tcp_conn_type.h"

void print_usage(const char *program_name)
{
    printf("用法: %s <配置文件路径>\n", program_name);
    printf("\n");
    printf("说明:\n");
    printf("  此工具用于加载TCP连接配置文件并格式化打印配置内容\n");
    printf("  支持的配置文件格式为JSON格式\n");
    printf("\n");
    printf("示例:\n");
    printf("  %s ../../utest/tcp_conn_loopback_config.json\n", program_name);
    printf("  %s ./test_config.json\n", program_name);
    printf("\n");
}

int main(int argc, char *argv[])
{
    if (argc != 2)
    {
        fprintf(stderr, "错误: 参数数量不正确\n\n");
        print_usage(argv[0]);
        return EXIT_FAILURE;
    }

    const char *config_file = argv[1];

    // 检查配置文件是否存在
    if (access(config_file, F_OK) != 0)
    {
        fprintf(stderr, "错误: 配置文件 '%s' 不存在或无法访问\n", config_file);
        return EXIT_FAILURE;
    }

    printf("正在加载配置文件: %s\n", config_file);

    // 分配配置结构体内存
    tcp_conn_mgr_settings_t settings;
    memset(&settings, 0, sizeof(settings));

    // 加载配置文件
    int result = load_tcp_config(config_file, &settings);
    if (result != 0)
    {
        fprintf(stderr, "错误: 加载配置文件失败，返回码: %d\n", result);
        return EXIT_FAILURE;
    }

    printf("配置文件加载成功！\n");

    // 格式化打印配置内容
    show_tcp_config(&settings);

    // 获取并显示操作接口类型信息
    const tcp_conn_manage_ops_t *ops = get_tcp_conn_ops_by_type(settings.type);
    if (ops != NULL)
    {
        printf("配置类型 '%s' 支持的操作接口: 可用\n", settings.type);
    }
    else
    {
        printf("警告: 配置类型 '%s' 不支持的操作接口\n", settings.type);
    }

    printf("配置文件加载和打印完成。\n");

    return EXIT_SUCCESS;
}
