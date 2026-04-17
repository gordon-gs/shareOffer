/*
 * @file     : ts_list.c
 * @brief    :
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-22 23:29:55
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */
#include "ts_list.h"
#include <stdlib.h>

void ts_list_init(ts_list_t *list, void (*free_func)(void *))
{
    list->head      = NULL;
    list->tail      = NULL;
    list->free_func = free_func;
    list->size      = 0;
    pthread_mutex_init(&list->lock, NULL);
}

void ts_list_destroy(ts_list_t *list)
{
    ts_list_clear(list);
    pthread_mutex_destroy(&list->lock);
}

void ts_list_clear(ts_list_t *list)
{
    pthread_mutex_lock(&list->lock);
    ts_node_t *cur = list->head;
    while (cur)
    {
        ts_node_t *tmp = cur;
        cur            = cur->next;
        if (list->free_func)
            list->free_func(tmp->data);
        free(tmp);
    }
    list->head = list->tail = NULL;
    list->size              = 0;
    pthread_mutex_unlock(&list->lock);
}

void ts_list_push_front(ts_list_t *list, void *data)
{
    ts_node_t *node = (ts_node_t *)malloc(sizeof(ts_node_t));
    node->data      = data;

    pthread_mutex_lock(&list->lock);
    node->next = list->head;
    list->head = node;
    if (!list->tail)
        list->tail = node;
    list->size++;
    pthread_mutex_unlock(&list->lock);
}

void ts_list_push_back(ts_list_t *list, void *data)
{
    ts_node_t *node = (ts_node_t *)malloc(sizeof(ts_node_t));
    node->data      = data;
    node->next      = NULL;

    pthread_mutex_lock(&list->lock);
    if (list->tail)
    {
        list->tail->next = node;
        list->tail       = node;
    }
    else
    {
        list->head = list->tail = node;
    }
    list->size++;
    pthread_mutex_unlock(&list->lock);
}

void *ts_list_pop_front(ts_list_t *list)
{
    pthread_mutex_lock(&list->lock);
    ts_node_t *node = list->head;
    if (!node)
    {
        pthread_mutex_unlock(&list->lock);
        return NULL;
    }

    list->head = node->next;
    if (!list->head)
        list->tail = NULL;

    void *data = node->data;
    free(node);
    list->size--;
    pthread_mutex_unlock(&list->lock);
    return data;
}

bool ts_list_remove(ts_list_t *list, void *target, bool (*cmp)(void *, void *))
{
    pthread_mutex_lock(&list->lock);
    ts_node_t *cur  = list->head;
    ts_node_t *prev = NULL;

    while (cur)
    {
        if (cmp(cur->data, target))
        {
            if (prev)
                prev->next = cur->next;
            else
                list->head = cur->next;

            if (cur == list->tail)
                list->tail = prev;

            if (list->free_func)
                list->free_func(cur->data);
            free(cur);
            list->size--;
            pthread_mutex_unlock(&list->lock);
            return true;
        }
        prev = cur;
        cur  = cur->next;
    }

    pthread_mutex_unlock(&list->lock);
    return false;
}

void *ts_list_find(ts_list_t *list, void *key, bool (*cmp)(void *, void *))
{
    pthread_mutex_lock(&list->lock);
    ts_node_t *cur = list->head;
    while (cur)
    {
        if (cmp(cur->data, key))
        {
            pthread_mutex_unlock(&list->lock);
            return cur->data;
        }
        cur = cur->next;
    }
    pthread_mutex_unlock(&list->lock);
    return NULL;
}

void ts_list_foreach(ts_list_t *list, void (*func)(void *data))
{
    pthread_mutex_lock(&list->lock);
    ts_node_t *cur = list->head;
    while (cur)
    {
        func(cur->data);
        cur = cur->next;
    }
    pthread_mutex_unlock(&list->lock);
}

void *ts_list_item_by_index(ts_list_t *list, size_t index)
{
    pthread_mutex_lock(&list->lock);

    ts_node_t *cur = list->head;
    size_t     i   = 0;
    while (cur)
    {
        if (i == index)
        {
            void *data = cur->data;
            pthread_mutex_unlock(&list->lock);
            return data;
        }
        cur = cur->next;
        i++;
    }

    pthread_mutex_unlock(&list->lock);
    return NULL;  // index 超出范围
}

int ts_list_size(ts_list_t *list)
{
    pthread_mutex_lock(&list->lock);
    int size = list->size;
    pthread_mutex_unlock(&list->lock);
    return size;
}
