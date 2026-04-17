#include "applog.h"

#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <pthread.h>
#include <sched.h>

#define LOG_QUEUE_SIZE 2048
#define LOG_MSG_MAX    1024

typedef struct
{
    applog_level_t level;
    char           message[LOG_MSG_MAX];
    char           file[64];
    int            line;
} log_msg_t;

static log_msg_t    log_queue[LOG_QUEUE_SIZE];
static unsigned int head = 0;
static unsigned int tail = 0;

static pthread_mutex_t write_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t  write_cond  = PTHREAD_COND_INITIALIZER;

static pthread_t    log_thread;
static volatile int running     = 0;
static int          cpu_to_bind = -1;  //! 性能调优时,避免调度至未隔离的CPU上

static applog_level_t current_level = APPLOG_INFO;

static const char *level_labels[] = {"[TRACE]", "[DEBUG]", " [INFO]", " [WARN]", "[ERROR]", "[FATAL]"};

// 用于同步线程启动
static pthread_mutex_t start_mutex    = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t  start_cond     = PTHREAD_COND_INITIALIZER;
static int             thread_started = 0;

static void *log_thread_func(void *arg)
{
    if (cpu_to_bind >= 0)
    {
        // cpu_set_t cpuset;
        // CPU_ZERO(&cpuset);
        // CPU_SET(cpu_to_bind, &cpuset);
        // pthread_setaffinity_np(pthread_self(), sizeof(cpu_set_t), &cpuset);
    }

    // 通知主线程，日志线程已启动
    pthread_mutex_lock(&start_mutex);
    thread_started = 1;
    pthread_cond_signal(&start_cond);
    pthread_mutex_unlock(&start_mutex);

    while (1)
    {
        pthread_mutex_lock(&write_mutex);

        // 等待有日志或者退出信号
        while (tail == head && running)
        {
            pthread_cond_wait(&write_cond, &write_mutex);
        }

        // 如果非运行状态且队列已空，退出线程
        if (!running && tail == head)
        {
            pthread_mutex_unlock(&write_mutex);
            break;
        }

        log_msg_t *msg = &log_queue[tail % LOG_QUEUE_SIZE];
        pthread_mutex_unlock(&write_mutex);

        // 获取纳秒时间
        struct timespec ts;
        clock_gettime(CLOCK_REALTIME, &ts);
        struct tm tm_info;
        localtime_r(&ts.tv_sec, &tm_info);

        char time_buf[64];
        strftime(time_buf, sizeof(time_buf), "%Y-%m-%d %H:%M:%S", &tm_info);
        char full_time_buf[80];
        snprintf(full_time_buf, sizeof(full_time_buf), "%s.%09ld", time_buf, ts.tv_nsec);
#if APPLOG_INCLUDE_FILE_LINE
        fprintf(
            stderr, "%s %s [%s:%d] %s\n", full_time_buf, level_labels[msg->level], msg->file, msg->line, msg->message);
#else
        fprintf(stderr, "%s %s %s\n", full_time_buf, level_labels[msg->level], msg->message);
#endif
        fflush(stderr);

        pthread_mutex_lock(&write_mutex);
        tail++;
        pthread_mutex_unlock(&write_mutex);
    }

    return NULL;
}

void applog_init(applog_level_t level, int log_cpu_id)
{
    current_level  = level;
    cpu_to_bind    = log_cpu_id;
    running        = 1;
    head           = 0;
    tail           = 0;
    thread_started = 0;

    pthread_create(&log_thread, NULL, log_thread_func, NULL);

    // 等待日志线程启动完成信号
    pthread_mutex_lock(&start_mutex);
    while (!thread_started)
        pthread_cond_wait(&start_cond, &start_mutex);
    pthread_mutex_unlock(&start_mutex);
}

void applog_shutdown(void)
{
    pthread_mutex_lock(&write_mutex);
    running = 0;
    pthread_cond_signal(&write_cond);
    pthread_mutex_unlock(&write_mutex);

    pthread_join(log_thread, NULL);
}

void applog_log_ex(applog_level_t level, const char *file, int line, const char *fmt, ...)
{
    if (level < current_level)
        return;

    pthread_mutex_lock(&write_mutex);
    unsigned int next_head = (head + 1) % LOG_QUEUE_SIZE;

    if (next_head == tail)
    {
        // 队列满，丢弃日志
        pthread_mutex_unlock(&write_mutex);
        return;
    }

    log_msg_t *msg = &log_queue[head];
    msg->level     = level;
    strncpy(msg->file, file, sizeof(msg->file) - 1);
    msg->file[sizeof(msg->file) - 1] = '\0';
    msg->line                        = line;

    va_list args;
    va_start(args, fmt);
    vsnprintf(msg->message, LOG_MSG_MAX, fmt, args);
    va_end(args);

    head = next_head;
    pthread_cond_signal(&write_cond);
    pthread_mutex_unlock(&write_mutex);
}

void applog_log(applog_level_t level, const char *fmt, ...)
{
    if (level < current_level)
        return;

    pthread_mutex_lock(&write_mutex);

    // 检查队列是否已满
    unsigned int next_head = (head + 1) % LOG_QUEUE_SIZE;
    if (next_head == tail)
    {
        pthread_mutex_unlock(&write_mutex);
        return;
    }

    // 填充日志消息
    log_msg_t *msg = &log_queue[head];
    msg->level     = level;

    va_list args;
    va_start(args, fmt);
    vsnprintf(msg->message, LOG_MSG_MAX, fmt, args);
    va_end(args);

    // 更新队列头
    head = next_head;

    // 通知日志处理线程
    pthread_cond_signal(&write_cond);
    pthread_mutex_unlock(&write_mutex);
}