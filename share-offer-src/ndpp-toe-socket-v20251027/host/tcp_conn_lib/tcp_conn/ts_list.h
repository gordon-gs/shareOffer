/**
 * @file     : ts_list.h
 * @brief    : 线程安全的链表
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-22 23:30:56
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#ifndef TS_LIST_H
#define TS_LIST_H

#include <stdbool.h>
#include <stddef.h>
#include <pthread.h>

typedef struct ts_node
{
    void           *data;
    struct ts_node *next;
} ts_node_t;

typedef struct
{
    ts_node_t      *head;
    ts_node_t      *tail;
    pthread_mutex_t lock;
    void (*free_func)(void *);
    int size;
} ts_list_t;

void ts_list_init(ts_list_t *list, void (*free_func)(void *));
void ts_list_destroy(ts_list_t *list);
void ts_list_clear(ts_list_t *list);

void  ts_list_push_front(ts_list_t *list, void *data);
void  ts_list_push_back(ts_list_t *list, void *data);
void *ts_list_pop_front(ts_list_t *list);

bool  ts_list_remove(ts_list_t *list, void *target, bool (*cmp)(void *, void *));
void *ts_list_find(ts_list_t *list, void *key, bool (*cmp)(void *, void *));
void  ts_list_foreach(ts_list_t *list, void (*func)(void *data));
void *ts_list_item_by_index(ts_list_t *list, size_t index);

int ts_list_size(ts_list_t *list);

#endif
