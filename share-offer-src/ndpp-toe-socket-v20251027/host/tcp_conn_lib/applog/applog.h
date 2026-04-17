#ifndef APPLOG_H
#define APPLOG_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdio.h>

//==================== 日志级别配置 ====================//

// 日志等级定义（从低到高）
typedef enum
{
    APPLOG_TRACE = 0,
    APPLOG_DEBUG,
    APPLOG_INFO,
    APPLOG_WARN,
    APPLOG_ERROR,
    APPLOG_FATAL,
    APPLOG_OFF  // 完全关闭日志
} applog_level_t;

// 当前日志编译级别控制：
// 例如：#define APPLOG_LEVEL APPLOG_INFO
// 表示只编译 INFO 及以上级别的日志
#ifndef APPLOG_LEVEL
#define APPLOG_LEVEL APPLOG_TRACE
#endif

// 是否包含文件名与行号
#define APPLOG_INCLUDE_FILE_LINE 1

//==================== 接口声明 ====================//

void applog_init(applog_level_t level, int log_cpu_id);
void applog_shutdown(void);
void applog_log_ex(applog_level_t level, const char *file, int line, const char *fmt, ...);
void applog_log(applog_level_t level, const char *fmt, ...);

//==================== 宏封装接口 ====================//

// 若 APPLOG_LEVEL == APPLOG_OFF，则所有日志完全禁用
#if APPLOG_LEVEL == APPLOG_OFF

#define LOG_TRACE(...) ((void)0)
#define LOG_DEBUG(...) ((void)0)
#define LOG_INFO(...)  ((void)0)
#define LOG_WARN(...)  ((void)0)
#define LOG_ERROR(...) ((void)0)
#define LOG_FATAL(...) ((void)0)

#else  // 启用日志，根据 APPLOG_LEVEL 过滤输出

#if APPLOG_INCLUDE_FILE_LINE
#if APPLOG_LEVEL <= APPLOG_TRACE
#define LOG_TRACE(...) applog_log_ex(APPLOG_TRACE, __FILE__, __LINE__, __VA_ARGS__)
#else
#define LOG_TRACE(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_DEBUG
#define LOG_DEBUG(...) applog_log_ex(APPLOG_DEBUG, __FILE__, __LINE__, __VA_ARGS__)
#else
#define LOG_DEBUG(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_INFO
#define LOG_INFO(...) applog_log_ex(APPLOG_INFO, __FILE__, __LINE__, __VA_ARGS__)
#else
#define LOG_INFO(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_WARN
#define LOG_WARN(...) applog_log_ex(APPLOG_WARN, __FILE__, __LINE__, __VA_ARGS__)
#else
#define LOG_WARN(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_ERROR
#define LOG_ERROR(...) applog_log_ex(APPLOG_ERROR, __FILE__, __LINE__, __VA_ARGS__)
#else
#define LOG_ERROR(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_FATAL
#define LOG_FATAL(...) applog_log_ex(APPLOG_FATAL, __FILE__, __LINE__, __VA_ARGS__)
#else
#define LOG_FATAL(...) ((void)0)
#endif

#else  // 不包含文件行号版本

#if APPLOG_LEVEL <= APPLOG_TRACE
#define LOG_TRACE(...) applog_log(APPLOG_TRACE, __VA_ARGS__)
#else
#define LOG_TRACE(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_DEBUG
#define LOG_DEBUG(...) applog_log(APPLOG_DEBUG, __VA_ARGS__)
#else
#define LOG_DEBUG(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_INFO
#define LOG_INFO(...) applog_log(APPLOG_INFO, __VA_ARGS__)
#else
#define LOG_INFO(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_WARN
#define LOG_WARN(...) applog_log(APPLOG_WARN, __VA_ARGS__)
#else
#define LOG_WARN(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_ERROR
#define LOG_ERROR(...) applog_log(APPLOG_ERROR, __VA_ARGS__)
#else
#define LOG_ERROR(...) ((void)0)
#endif

#if APPLOG_LEVEL <= APPLOG_FATAL
#define LOG_FATAL(...) applog_log(APPLOG_FATAL, __VA_ARGS__)
#else
#define LOG_FATAL(...) ((void)0)
#endif

#endif  // APPLOG_INCLUDE_FILE_LINE

#endif  // APPLOG_LEVEL == APPLOG_OFF

#ifdef __cplusplus
}
#endif

#endif  // APPLOG_H
