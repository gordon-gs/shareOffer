/*
 * @file     : ringbuff.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-07-12 22:28:09
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "ringbuff.h"
#include "stdafx.h"

/*
 * 用 __atomic_load_n 和 __atomic_store_n 来保证原子访问和内存序，
 * memory_order_acquire 用于读取（消费者读对方指针）
 * memory_order_release 用于写入（生产者写自己的指针）
 */

int ringbuff_init(ringbuff_t *rb, size_t size)
{
    if (!rb || size < 2)
        return -1;

    rb->data = (uint8_t *)malloc(size);
    if (!rb->data)
        return -1;

    rb->size = size;
    __atomic_store_n(&rb->head, 0, __ATOMIC_RELAXED);
    __atomic_store_n(&rb->tail, 0, __ATOMIC_RELAXED);
    rb->full = 0;
    memset(rb->_pad, 0, sizeof(rb->_pad));

    return 0;
}

void ringbuff_free(ringbuff_t *rb)
{
    if (rb && rb->data)
    {
        free(rb->data);
        rb->data = NULL;
    }
}

void ringbuff_reset(ringbuff_t *rb)
{
    if (!rb)
        return;

    __atomic_store_n(&rb->head, 0, __ATOMIC_RELAXED);
    __atomic_store_n(&rb->tail, 0, __ATOMIC_RELAXED);
    rb->full = 0;
}

size_t ringbuff_data_len(const ringbuff_t *rb)
{
    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_ACQUIRE);

    if (rb->full)
        return rb->size;

    if (head >= tail)
        return head - tail;
    else
        return rb->size - tail + head;
}

size_t ringbuff_free_space(const ringbuff_t *rb)
{
    return rb->size - ringbuff_data_len(rb);
}

int ringbuff_write(ringbuff_t *rb, const void *src, size_t len)
{
    if (!rb || !src || len == 0)
        return 0;

    size_t head = __atomic_load_n(&rb->head, __ATOMIC_RELAXED);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_ACQUIRE);

    size_t free_space;
    if (rb->full)
    {
        free_space = 0;
    }
    else if (tail > head)
    {
        free_space = tail - head;
    }
    else
    {
        free_space = rb->size - head + tail;
    }

    if (len > free_space)
        len = free_space;

    if (len == 0)
        return 0;

    size_t first_part = rb->size - head;
    if (first_part > len)
        first_part = len;

    memcpy(rb->data + head, src, first_part);
    memcpy(rb->data, (const uint8_t *)src + first_part, len - first_part);

    head = (head + len) % rb->size;
    __atomic_store_n(&rb->head, head, __ATOMIC_RELEASE);

    rb->full = (head == tail) ? 1 : 0;

    return (int)len;
}

int ringbuff_read_ptr(ringbuff_t *rb, const void **ptr, size_t *len)
{
    if (!rb || !ptr || !len)
        return -1;

    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);

    size_t data_len;
    if (rb->full)
    {
        data_len = rb->size;
    }
    else if (head >= tail)
    {
        data_len = head - tail;
    }
    else
    {
        data_len = rb->size - tail + head;
    }

    if (data_len == 0)
        return -2;

    size_t max_read = rb->size - tail;
    if (data_len < max_read)
        max_read = data_len;

    *ptr = rb->data + tail;
    *len = max_read;

    return 0;
}

void ringbuff_consume(ringbuff_t *rb, size_t len)
{
    if (!rb || len == 0)
        return;

    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);

    size_t data_len;
    if (rb->full)
    {
        data_len = rb->size;
    }
    else if (head >= tail)
    {
        data_len = head - tail;
    }
    else
    {
        data_len = rb->size - tail + head;
    }

    if (len >= data_len)
    {
        __atomic_store_n(&rb->tail, 0, __ATOMIC_RELEASE);
        __atomic_store_n(&rb->head, 0, __ATOMIC_RELAXED);
        rb->full = 0;
        return;
    }

    tail = (tail + len) % rb->size;
    __atomic_store_n(&rb->tail, tail, __ATOMIC_RELEASE);
    rb->full = 0;
}

int ringbuff_read(ringbuff_t *rb, void *dst, size_t len)
{
    if (!rb || !dst || len == 0)
        return 0;

    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);

    size_t data_len;
    if (rb->full)
    {
        data_len = rb->size;
    }
    else if (head >= tail)
    {
        data_len = head - tail;
    }
    else
    {
        data_len = rb->size - tail + head;
    }

    if (len > data_len)
        len = data_len;

    if (len == 0)
        return 0;

    size_t first_part = rb->size - tail;
    if (first_part > len)
        first_part = len;

    memcpy(dst, rb->data + tail, first_part);
    memcpy((uint8_t *)dst + first_part, rb->data, len - first_part);

    tail = (tail + len) % rb->size;
    __atomic_store_n(&rb->tail, tail, __ATOMIC_RELEASE);
    rb->full = 0;

    return (int)len;
}

int ringbuff_peek(ringbuff_t *rb, void *dst, size_t len)
{
    if (!rb || !dst || len == 0)
        return 0;

    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);

    size_t data_len;
    if (rb->full)
    {
        data_len = rb->size;
    }
    else if (head >= tail)
    {
        data_len = head - tail;
    }
    else
    {
        data_len = rb->size - tail + head;
    }

    if (len > data_len)
        len = data_len;

    if (len == 0)
        return 0;

    size_t first_part = rb->size - tail;
    if (first_part > len)
        first_part = len;

    memcpy(dst, rb->data + tail, first_part);
    memcpy((uint8_t *)dst + first_part, rb->data, len - first_part);

    return (int)len;
}
