/**
 * @file     : tcp_conn_define.h
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-25 12:13:27
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#pragma once

// 可从配置中加载的最大TCP连接数量
#define MAX_CONN (16)
// TX/RX缓冲区配置的最小值(在配置文件中可扩大)
#define RING_BUFFER_SIZE (8 * 1024UL)

// 内部参数: RX/TX 线程配置的事件数量
#define MAX_EVENTS             128

#define CONNECTION_TIMEOUT_SEC 60
