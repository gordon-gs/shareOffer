/**
 * @file     : tcp_conn_version.h
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-21 14:16:59
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#ifndef TCP_CONN_VERSION_H
#define TCP_CONN_VERSION_H

#define TCP_CONN_VERSION_V 0
#define TCP_CONN_VERSION_R 2
#define TCP_CONN_VERSION_P 0

#define STR_HELPER(x) #x
#define STR(x)        STR_HELPER(x)

#define TCP_CONN_VERSION_TEXT STR(TCP_CONN_VERSION_V) "." STR(TCP_CONN_VERSION_R) "." STR(TCP_CONN_VERSION_P)

#endif  // TCP_CONN_VERSION_H