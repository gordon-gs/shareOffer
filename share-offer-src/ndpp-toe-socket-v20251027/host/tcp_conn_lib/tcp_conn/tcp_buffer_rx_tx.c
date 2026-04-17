/*
 * @file     : tcp_buffer_rx_tx.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-20 20:43:40
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "tcp_conn_common.h"
#include "stdafx.h"

/* RX BUFFER */
int rx_buffer_init(rx_buffer_t *rb, size_t size)
{
    if (!rb || pipe(rb->pipe_fd) != 0)
        return -1;
    if (ringbuff_init(&rb->buffer, size) != 0)
    {
        close(rb->pipe_fd[0]);
        close(rb->pipe_fd[1]);
        return -1;
    }
    fcntl(rb->pipe_fd[0], F_SETFL, O_NONBLOCK);
    fcntl(rb->pipe_fd[1], F_SETFL, O_NONBLOCK);
    pthread_mutex_init(&rb->lock, NULL);
    rb->ready = false;
    return 0;
}

void rx_buffer_destroy(rx_buffer_t *rb)
{
    if (!rb)
        return;
    close(rb->pipe_fd[0]);
    close(rb->pipe_fd[1]);
    ringbuff_free(&rb->buffer);
    pthread_mutex_destroy(&rb->lock);
}

ssize_t rx_buffer_write(rx_buffer_t *tb, const void *data, size_t len)
{
    if (!tb || !data || len == 0)
        return -1;
    pthread_mutex_lock(&tb->lock);

    uint32_t header = (uint32_t)len;
    size_t   total  = sizeof(header) + len;
    if (ringbuff_free_space(&tb->buffer) < total)
    {
        pthread_mutex_unlock(&tb->lock);
        return 0;
    }
    ringbuff_write(&tb->buffer, data, len);
    tb->ready = true;

    pthread_mutex_unlock(&tb->lock);
    return len;
}

ssize_t rx_buffer_read(rx_buffer_t *rb, void *dst, size_t len)
{
    if (!rb || !dst || len == 0)
        return -1;
    pthread_mutex_lock(&rb->lock);
    char tmp;
    read(rb->pipe_fd[0], &tmp, 1);
    ssize_t n = ringbuff_read(&rb->buffer, dst, len);
    rb->ready = (ringbuff_data_len(&rb->buffer) > 0);
    pthread_mutex_unlock(&rb->lock);
    return n;
}

int rx_buffer_peek(rx_buffer_t *rb, const void **ptr, size_t *len)
{
    if (!rb)
        return -1;
    pthread_mutex_lock(&rb->lock);
    int r = ringbuff_read_ptr(&rb->buffer, ptr, len);
    pthread_mutex_unlock(&rb->lock);
    return r;
}

void rx_buffer_consume(rx_buffer_t *rb, size_t len)
{
    if (!rb)
        return;
    pthread_mutex_lock(&rb->lock);
    ringbuff_consume(&rb->buffer, len);
    rb->ready = (ringbuff_data_len(&rb->buffer) > 0);
    pthread_mutex_unlock(&rb->lock);
}

int rx_buffer_fd(rx_buffer_t *rb)
{
    return rb ? rb->pipe_fd[0] : -1;
}

bool rx_buffer_data_ready(rx_buffer_t *rb)
{
    return rb ? rb->ready : false;
}

/* TX BUFFER */
int tx_buffer_init(tx_buffer_t *tb, size_t size)
{
    if (!tb || pipe(tb->pipe_fd) != 0)
        return -1;
    if (ringbuff_init(&tb->buffer, size) != 0)
    {
        close(tb->pipe_fd[0]);
        close(tb->pipe_fd[1]);
        return -1;
    }
    fcntl(tb->pipe_fd[0], F_SETFL, O_NONBLOCK);
    fcntl(tb->pipe_fd[1], F_SETFL, O_NONBLOCK);
    pthread_mutex_init(&tb->lock, NULL);
    tb->ready = false;
    return 0;
}

void tx_buffer_destroy(tx_buffer_t *tb)
{
    if (!tb)
        return;
    // close(tb->pipe_fd[0]);
    // close(tb->pipe_fd[1]);
    ringbuff_free(&tb->buffer);
    pthread_mutex_destroy(&tb->lock);
}

ssize_t tx_buffer_write(tx_buffer_t *tb, const void *data, size_t len)
{
    if (!tb || !data || len == 0)
        return -1;
    pthread_mutex_lock(&tb->lock);

    uint32_t header = (uint32_t)len;
    size_t   total  = sizeof(header) + len;
    size_t   free_size = ringbuff_free_space(&tb->buffer);
    if (free_size < total)
    {
        pthread_mutex_unlock(&tb->lock);
        return -free_size;
    }
    ringbuff_write(&tb->buffer, &header, sizeof(header));
    ringbuff_write(&tb->buffer, data, len);
    tb->ready = true;
    pthread_mutex_unlock(&tb->lock);
    return len;
}

int tx_buffer_peek_next(tx_buffer_t *tb, const void **ptr, size_t *len)
{
    if (!tb)
        return -1;
    pthread_mutex_lock(&tb->lock);
    const void *hdr_ptr;
    size_t      hdr_len;
    if (ringbuff_read_ptr(&tb->buffer, &hdr_ptr, &hdr_len) != 0 || hdr_len < sizeof(uint32_t))
    {
        pthread_mutex_unlock(&tb->lock);
        return -1;
    }
    uint32_t msg_len;
    memcpy(&msg_len, hdr_ptr, sizeof(uint32_t));

    ringbuff_consume(&tb->buffer, sizeof(uint32_t));
    ringbuff_read_ptr(&tb->buffer, ptr, len);
    *len = msg_len;
    pthread_mutex_unlock(&tb->lock);
    return 0;
}

void tx_buffer_consume(tx_buffer_t *tb, size_t len)
{
    if (!tb || len <= 0)
        return;

    pthread_mutex_lock(&tb->lock);

    size_t available = ringbuff_data_len(&tb->buffer);
    if (available >= len)
    {
        ringbuff_consume(&tb->buffer, len);
    }
    else
    {
        ringbuff_consume(&tb->buffer, available);
    }

    tb->ready = (ringbuff_data_len(&tb->buffer) > 0);

    pthread_mutex_unlock(&tb->lock);
}

int tx_buffer_fd(tx_buffer_t *tb)
{
    return tb ? tb->pipe_fd[0] : -1;
}

bool tx_buffer_data_ready(tx_buffer_t *tb)
{
    return tb ? tb->ready : false;
}

/* 新增的辅助函数 */
size_t rx_buffer_get_used_size(rx_buffer_t *rb)
{
    if (!rb)
        return 0;
    pthread_mutex_lock(&rb->lock);
    size_t size = ringbuff_data_len(&rb->buffer);
    pthread_mutex_unlock(&rb->lock);
    return size;
}

size_t tx_buffer_get_used_size(tx_buffer_t *tb)
{
    if (!tb)
        return 0;
    pthread_mutex_lock(&tb->lock);
    size_t size = ringbuff_data_len(&tb->buffer);
    pthread_mutex_unlock(&tb->lock);
    return size;
}

size_t tx_buffer_get_free_size(tx_buffer_t *tb)
{
    if (!tb)
        return 0;
    pthread_mutex_lock(&tb->lock);
    size_t size = ringbuff_free_space(&tb->buffer);
    pthread_mutex_unlock(&tb->lock);
    return size;
}

uint64_t rx_buffer_get_total_received(rx_buffer_t *rb)
{
    if (!rb)
        return 0;
    // TODO: 实现统计功能
    return 0;
}

uint64_t tx_buffer_get_total_sent(tx_buffer_t *tb)
{
    if (!tb)
        return 0;
    // TODO: 实现统计功能
    return 0;
}
