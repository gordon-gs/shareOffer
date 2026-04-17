/*
 * @file     : tcp_conn_config.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 13:57:53
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "tcp_conn_common.h"
#include "tcp_conn_instanta.h"
#include "tcp_conn_toe.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/ioctl.h>
#include <net/if.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <net/if_arp.h>
//! 遍历arp_tbl中描述的ip列表,尝试用本地网卡的arp列表进行解析绑定,并且标识出哪些属于本地
//! 使用 0.0.0.0 的本地端口的TCP Server将从配置自动绑定对应的本地IP,
//! 目前简化为总是用 0 监听端口,只响应配置列表中提供的 IP 作为合法的客户端

static int update_arp_table(arp_entry_t *arp_tbl, const int cnts)
{
    if (arp_tbl == NULL || cnts <= 0)
    {
        return -1;
    }

    int sockfd = socket(AF_INET, SOCK_DGRAM, 0);
    if (sockfd < 0)
    {
        perror("socket");
        return -1;
    }

    // 获取本地网络接口信息
    struct ifconf ifc;
    char          buf[8192];
    int           success = 0;

    ifc.ifc_len = sizeof(buf);
    ifc.ifc_buf = buf;
    if (ioctl(sockfd, SIOCGIFCONF, &ifc) < 0)
    {
        perror("ioctl(SIOCGIFCONF)");
        close(sockfd);
        return -1;
    }

    // 遍历所有网络接口
    struct ifreq *ifr         = ifc.ifc_req;
    int           ninterfaces = ifc.ifc_len / sizeof(struct ifreq);

    for (int i = 0; i < ninterfaces; i++)
    {
        struct ifreq *item = &ifr[i];

        // 获取接口的MAC地址
        if (ioctl(sockfd, SIOCGIFHWADDR, item) < 0)
        {
            continue;
        }

        // 检查是否为ARP类型（以太网）
        if (item->ifr_hwaddr.sa_family != ARPHRD_ETHER)
        {
            continue;
        }

        // 获取接口的IP地址
        if (ioctl(sockfd, SIOCGIFADDR, item) < 0)
        {
            continue;
        }

        struct sockaddr_in *ipaddr = (struct sockaddr_in *)&item->ifr_addr;
        char                ip_str[INET_ADDRSTRLEN];
        inet_ntop(AF_INET, &ipaddr->sin_addr, ip_str, sizeof(ip_str));

        // 获取MAC地址字符串
        unsigned char *mac = (unsigned char *)item->ifr_hwaddr.sa_data;
        char           mac_str[18];
        snprintf(
            mac_str, sizeof(mac_str), "%02x:%02x:%02x:%02x:%02x:%02x", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

        // 检查这个IP是否在目标ARP表中
        for (int j = 0; j < cnts; j++)
        {
            if (strcmp(arp_tbl[j].host_ip, ip_str) == 0)
            {
                // 更新MAC地址
                strncpy(arp_tbl[j].host_mac, mac_str, sizeof(arp_tbl[j].host_mac) - 1);
                arp_tbl[j].host_mac[sizeof(arp_tbl[j].host_mac) - 1] = '\0';

                // 标记为本地主机
                arp_tbl[j].is_local = 1;
                success++;
                break;
            }
        }
    }

    close(sockfd);

    // 对于未找到本地接口的条目，尝试解析MAC地址
    for (int i = 0; i < cnts; i++)
    {
        if ((arp_tbl[i].host_mac[0] == '\0' || arp_tbl[i].host_mac[0] == '0'))
        {
            // 这里可以添加ARP请求来解析远程主机的MAC地址
            // 目前简化处理，只标记为非本地
            arp_tbl[i].is_local = 0;
        }
    }

    return success;
}

int load_tcp_config(const char *filename, tcp_conn_mgr_settings_t *settings)
{
    struct json_object *root = json_object_from_file(filename);
    if (!root)
    {
        fprintf(stderr, "Failed to open or parse JSON file: %s\n", filename);
        return -1;
    }

    // version / description
    struct json_object *ver, *desc;
    if (json_object_object_get_ex(root, "version", &ver))
        snprintf(settings->version, sizeof(settings->version), "%s", json_object_get_string(ver));
    if (json_object_object_get_ex(root, "description", &desc))
        snprintf(settings->description, sizeof(settings->description), "%s", json_object_get_string(desc));
    if (json_object_object_get_ex(root, "type", &desc))
        snprintf(settings->type, sizeof(settings->type), "%s", json_object_get_string(desc));

    // global_settings
    struct json_object *gobj;
    if (json_object_object_get_ex(root, "global_settings", &gobj))
    {
        struct json_object *tmp;
        if (json_object_object_get_ex(gobj, "max_connections", &tmp))
            settings->global.max_connections = json_object_get_int(tmp);
        if (json_object_object_get_ex(gobj, "ring_buffer_size", &tmp))
            settings->global.ring_buffer_size = json_object_get_int(tmp);
        if (json_object_object_get_ex(gobj, "reconnect_interval", &tmp))
            settings->global.reconnect_interval = json_object_get_int(tmp);
        if (json_object_object_get_ex(gobj, "connection_timeout", &tmp))
            settings->global.connection_timeout = json_object_get_int(tmp);
    }

    // ARP 表
    struct json_object *arp_arr;
    if (json_object_object_get_ex(root, "arp_table", &arp_arr))
    {
        int arp_len         = json_object_array_length(arp_arr);
        settings->arp_count = arp_len;
        for (int i = 0; i < arp_len; i++)
        {
            struct json_object *entry = json_object_array_get_idx(arp_arr, i);
            struct json_object *mac, *ip, *is_local;

            json_object_object_get_ex(entry, "host_mac", &mac);
            json_object_object_get_ex(entry, "host_ip", &ip);
            json_object_object_get_ex(entry, "is_local", &is_local);

            snprintf(
                settings->arp_table[i].host_mac,
                sizeof(settings->arp_table[i].host_mac),
                "%s",
                json_object_get_string(mac));
            snprintf(
                settings->arp_table[i].host_ip,
                sizeof(settings->arp_table[i].host_ip),
                "%s",
                json_object_get_string(ip));
            settings->arp_table[i].is_local = is_local ? json_object_get_boolean(is_local) : 0;
        }
        if (arp_len)
            update_arp_table(settings->arp_table, arp_len);
    }

    // connections
    struct json_object *conn_arr;
    if (json_object_object_get_ex(root, "connections", &conn_arr))
    {
        int conn_len         = json_object_array_length(conn_arr);
        settings->conn_count = conn_len;

        for (int i = 0; i < conn_len; i++)
        {
            struct json_object *conn = json_object_array_get_idx(conn_arr, i);
            struct json_object *id, *tag, *type;
            struct json_object *local_ip, *local_port;
            struct json_object *remote_ip, *remote_port;

            tcp_conn_info_t *ci = &settings->conn_list[i];
            memset(ci, 0, sizeof(tcp_conn_info_t));

            json_object_object_get_ex(conn, "conn_id", &id);
            json_object_object_get_ex(conn, "conn_tag", &tag);
            json_object_object_get_ex(conn, "conn_type", &type);
            json_object_object_get_ex(conn, "local_ip", &local_ip);
            json_object_object_get_ex(conn, "local_port", &local_port);
            json_object_object_get_ex(conn, "remote_ip", &remote_ip);
            json_object_object_get_ex(conn, "remote_port", &remote_port);

            // 填充连接信息
            ci->conn_id = id ? json_object_get_int(id) : 0;
            if (tag)
                snprintf(ci->conn_tag, sizeof(ci->conn_tag), "%s", json_object_get_string(tag));

            const char *type_str = type ? json_object_get_string(type) : "client";
            ci->conn_type        = (strcmp(type_str, "server") == 0) ? CONN_TYPE_SERVER : CONN_TYPE_CLIENT;

            ci->conn_enabled = 1;

            if (local_ip)
                snprintf(ci->local_ip, sizeof(ci->local_ip), "%s", json_object_get_string(local_ip));
            ci->local_port = local_port ? json_object_get_int(local_port) : 0;

            if (remote_ip)
                snprintf(ci->remote_ip, sizeof(ci->remote_ip), "%s", json_object_get_string(remote_ip));
            ci->remote_port = remote_port ? json_object_get_int(remote_port) : 0;

            ci->rx_buffer_size = settings->global.ring_buffer_size;
            ci->tx_buffer_size = settings->global.ring_buffer_size;

            ci->conn_state = CONN_STATE_NONE;

            ci->rx_pipe_fd     = -1;
            ci->tx_pipe_fd     = -1;
            ci->rx_buffer_used = 0;
            ci->tx_buffer_used = 0;
        }
    }

    json_object_put(root);
    return 0;
}

void show_tcp_config(tcp_conn_mgr_settings_t *settings)
{
    if (!settings)
        return;

    printf("\n================ TCP CONFIGURATION ================\n");
    printf(" Version      : %s\n", settings->version);
    printf(" Description  : %s\n", settings->description);
    printf(" Type         : %s\n", settings->type);
    printf("---------------------------------------------------\n");
    printf(" Global Settings:\n");
    printf("   Max Connections   : %d\n", settings->global.max_connections);
    printf("   Ring Buffer Size  : %d bytes\n", settings->global.ring_buffer_size);
    printf("   Reconnect Interval: %d\n", settings->global.reconnect_interval);
    printf("   Connection Timeout: %d\n", settings->global.connection_timeout);
    printf("---------------------------------------------------\n");

    printf(" ARP Table (%d entries):\n", settings->arp_count);
    for (int i = 0; i < settings->arp_count; i++)
    {
        printf(
            "   [%02d] IP: %-15s  MAC: %s  Local: %s\n",
            i,
            settings->arp_table[i].host_ip,
            settings->arp_table[i].host_mac,
            settings->arp_table[i].is_local ? "Yes" : "No");
    }
    printf("---------------------------------------------------\n");

    printf(" Connections (%d total):\n", settings->conn_count);
    for (int i = 0; i < settings->conn_count; i++)
    {
        tcp_conn_info_t *ci = &settings->conn_list[i];
        printf(" [%02d] %s\n", ci->conn_id, ci->conn_tag);
        printf("      Type       : %s\n", conn_type_str(ci->conn_type));
        printf("      Enabled    : %s\n", ci->conn_enabled ? "YES" : "NO");
        printf("      State      : %s\n", conn_state_str(ci->conn_state));
        printf("      Local      : %-15s:%-5d (%s)\n", ci->local_ip, ci->local_port, ci->local_mac);
        printf("      Remote     : %-15s:%-5d (%s)\n", ci->remote_ip, ci->remote_port, ci->remote_mac);
        printf("      RX Buffer  : %d / %d bytes\n", ci->rx_buffer_used, ci->rx_buffer_size);
        printf("      TX Buffer  : %d / %d bytes\n", ci->tx_buffer_used, ci->tx_buffer_size);
        printf("---------------------------------------------------\n");
    }

    printf("================ END OF CONFIG ====================\n\n");
}

/**
 * @brief 根据配置文件类型获取对应的TCP连接管理操作接口
 * @param type 配置文件类型字符串 ("socket", "toe", 等)
 * @return 返回对应的操作接口指针，如果类型不支持则返回NULL
 */
const tcp_conn_manage_ops_t *get_tcp_conn_ops_by_type(const char *type)
{
    if (!type)
        return NULL;

    if (strcmp(type, "socket") == 0)
    {
        return &socket_conn_ops;
    }
    else if (strcmp(type, "instanta") == 0)
    {
        return &socket_conn_ops;
    }
    else if (strcmp(type, "toe") == 0)
    {
        return &toe_tcp_conn_ops;
    }
    else
    {
        fprintf(stderr, "Unsupported TCP connection type: %s\n", type);
        fprintf(stderr, "Supported types: socket, instanta, toe\n");
        return NULL;
    }
}

/**
 * @brief 加载配置文件并返回对应的操作接口
 * @param filename 配置文件路径
 * @param settings 输出的配置结构体
 * @return 返回对应的操作接口指针，如果失败则返回NULL
 */
const tcp_conn_manage_ops_t *load_tcp_config_with_ops(const char *filename, tcp_conn_mgr_settings_t *settings)
{
    if (load_tcp_config(filename, settings) != 0)
    {
        return NULL;
    }

    return get_tcp_conn_ops_by_type(settings->type);
}
