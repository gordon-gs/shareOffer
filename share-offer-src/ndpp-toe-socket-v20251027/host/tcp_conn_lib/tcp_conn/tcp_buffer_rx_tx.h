/**
 * @file     : tcp_buffer_rx_tx.h
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-25 17:31:28
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#pragma once
#include "ringbuff.h"
#include <pthread.h>
#include <stdbool.h>
#include <unistd.h>
typedef struct rx_buffer_s
{
    int             pipe_fd[2];
    ringbuff_t      buffer;
    pthread_mutex_t lock;
    bool            ready;
} rx_buffer_t;

int     rx_buffer_init(rx_buffer_t *rb, size_t size);
void    rx_buffer_destroy(rx_buffer_t *rb);
ssize_t rx_buffer_write(rx_buffer_t *tb, const void *data, size_t len);
int     rx_buffer_peek(rx_buffer_t *rb, const void **ptr, size_t *len);
void    rx_buffer_consume(rx_buffer_t *rb, size_t len);
ssize_t rx_buffer_read(rx_buffer_t *rb, void *dst, size_t len);
int     rx_buffer_fd(rx_buffer_t *rb);
bool    rx_buffer_data_ready(rx_buffer_t *rb);

//! 按设计要求, rx_buffer 中 payload 连续存放,而 tx_buffer 不连续存放, 还附加用于控制的报头
typedef struct tx_buffer_s
{
    int             pipe_fd[2];
    ringbuff_t      buffer;
    pthread_mutex_t lock;
    bool            ready;
} tx_buffer_t;

int     tx_buffer_init(tx_buffer_t *tb, size_t size);
void    tx_buffer_destroy(tx_buffer_t *tb);
ssize_t tx_buffer_write(tx_buffer_t *tb, const void *data, size_t len);
int     tx_buffer_peek_next(tx_buffer_t *tb, const void **ptr, size_t *len);
void    tx_buffer_consume(tx_buffer_t *tb, size_t len);
int     tx_buffer_fd(tx_buffer_t *tb);
bool    tx_buffer_data_ready(tx_buffer_t *tb);

/* 新增的辅助函数 */
size_t  rx_buffer_get_used_size(rx_buffer_t *rb);
size_t  tx_buffer_get_used_size(tx_buffer_t *tb);
size_t  tx_buffer_get_free_size(tx_buffer_t *tb);
uint64_t rx_buffer_get_total_received(rx_buffer_t *rb);
uint64_t tx_buffer_get_total_sent(tx_buffer_t *tb);
