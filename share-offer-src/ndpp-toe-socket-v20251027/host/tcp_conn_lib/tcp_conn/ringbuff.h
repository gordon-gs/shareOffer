#ifndef RINGBUFF_H
#define RINGBUFF_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct
{
    uint8_t        *data;
    size_t          size;
    volatile size_t head;  // 写入位置
    volatile size_t tail;  // 读取位置
    int             full;
    char            _pad[64 - (sizeof(size_t) * 2 + sizeof(int))];  // 缓存行填充
} ringbuff_t;

// 初始化
int ringbuff_init(ringbuff_t *rb, size_t size);

// 释放
void ringbuff_free(ringbuff_t *rb);

// 写入数据，返回写入的字节数
int ringbuff_write(ringbuff_t *rb, const void *src, size_t len);

// 读取指针，不复制，获取连续可读块
int ringbuff_read_ptr(ringbuff_t *rb, const void **ptr, size_t *len);

// 消费数据（读取后前移）
void ringbuff_consume(ringbuff_t *rb, size_t len);

// 复制读取数据（不推荐用于高性能路径）
int ringbuff_read(ringbuff_t *rb, void *dst, size_t len);

// 只 peek 数据，但不前移
int ringbuff_peek(ringbuff_t *rb, void *dst, size_t len);

// 缓冲区剩余空间
size_t ringbuff_free_space(const ringbuff_t *rb);

// 缓冲区当前数据长度
size_t ringbuff_data_len(const ringbuff_t *rb);

// 清空缓冲区
void ringbuff_reset(ringbuff_t *rb);

#ifdef __cplusplus
}
#endif

#endif  // RINGBUFF_H
