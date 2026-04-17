/*
 * @file     : tcp_conn_misc.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-24 00:32:41
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "tcp_conn_common.h"
int set_sock_timeout(int sockfd, int timeout_ms)
{
    struct timeval timeout;
    timeout.tv_sec  = timeout_ms / 1000;
    timeout.tv_usec = (timeout_ms % 1000) * 1000UL;
    if (setsockopt(sockfd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout)) < 0)
    {
        perror("setsockopt SO_SNDTIMEO");
        close(sockfd);
        return -1;
    }
    if (setsockopt(sockfd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout)) < 0)
    {
        perror("setsockopt SO_RCVTIMEO");
        close(sockfd);
        return -2;
    }
    return 0;
}

void dump_bytes_array(void *data, size_t len, size_t offset)
{
    unsigned char *ptr       = (unsigned char *)data;
    size_t         total_len = offset + len;

    for (size_t i = 0; i < total_len; i += 16)
    {
        printf("        0x%04zx:  ", i);

        size_t start_pos = 0;
        if (i < offset)
        {
            if ((i + 16) >= offset)
            {
                start_pos = offset - i;
            }
            else
            {
                start_pos = 16;
            }
        }

        for (size_t j = 0; j < start_pos; j++)
        {
            printf("  ");
            if (j % 2)
                printf(" ");
        }
        for (size_t j = start_pos; j < 16; j++)
        {
            size_t data_index = (i - offset) + j;

            if (data_index < len)
            {
                printf("%02x", ptr[data_index]);
            }
            else
            {
                printf("  ");
            }

            if (j % 2)
                printf(" ");
        }

        printf("\n");
    }
}
