/*
 * @file     : tcp_conn_instanta.c
 * @brief    : TCP连接管理实现 - 基于epoll的异步I/O处理
 * @author   : chenshaxiong (chensx@yusur.tech)
 * @date     : 2025-10-26 22:58:38
 * @copyright: Copyright(c) 2022-2024, YUSUR Technology Co., Ltd. Learn more at www.yusur.tech.
 */

#include "tcp_conn.h"
#include "tcp_conn_common.h"
#include "tcp_conn_instanta.h"
#include "stdafx.h"
#include <sys/epoll.h>
#include <netinet/tcp.h>

static void *rx_thread_func(void *arg);
static void *tx_thread_func(void *arg);

/**
 * @brief 设置socket为非阻塞模式
 * @param fd 文件描述符
 * @return 成功返回0，失败返回-1
 */
int set_sock_nonblock(int fd)
{
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags == -1)
    {
        return -1;
    }
    return fcntl(fd, F_SETFL, flags | O_NONBLOCK);
}

/**
 * @brief 更新连接状态并发送相应事件
 * @param conn 连接对象
 * @param new_state 新状态
 * @param error_code 错误码（如果有错误）
 */
static void update_conn_state(tcp_conn_item_t *conn, conn_state_t new_state, int error_code)
{
    if (!conn)
        return;

    conn_state_t old_state = conn->state;
    conn->state = new_state;

    // 根据状态变化发送相应事件
    switch (new_state)
    {
    case CONN_STATE_CONNECTED:
        {
            // clang-format off
            tcp_conn_event_t evt = {
                .conn_id = conn->conn_id,
                .type = TCP_EVENT_CONNECTED,
                .resv = error_code
            };
            // clang-format on
            int     pipefd = conn->rx_buf.pipe_fd[1];
            ssize_t rc     = write(pipefd, &evt, sizeof(evt));
            if (rc != sizeof(evt))
            {
                LOG_WARN("Failed to send connected event for conn[%d]\n", conn->conn_id);
            }
            else
            {
                LOG_DEBUG("Sent connected event for conn[%d]\n", conn->conn_id);
            }
        }
        break;

    case CONN_STATE_CLOSING:
        {
            tcp_conn_event_t evt = {.conn_id = conn->conn_id, .type = TCP_EVENT_CLOSING};
            int pipefd = conn->rx_buf.pipe_fd[1];
            ssize_t rc = write(pipefd, &evt, sizeof(evt));
            if (rc != sizeof(evt))
            {
                LOG_WARN("Failed to send closing event for conn[%d]\n", conn->conn_id);
            }
            else
            {
                LOG_DEBUG("Sent closing event for conn[%d]\n", conn->conn_id);
            }
        }
        break;

    case CONN_STATE_CLOSED:
        {
            tcp_conn_event_t evt = {.conn_id = conn->conn_id, .type = TCP_EVENT_CLOSED};
            int pipefd = conn->rx_buf.pipe_fd[1];
            ssize_t rc = write(pipefd, &evt, sizeof(evt));
            if (rc != sizeof(evt))
            {
                LOG_WARN("Failed to send closed event for conn[%d]\n", conn->conn_id);
            }
            else
            {
                LOG_DEBUG("Sent closed event for conn[%d]\n", conn->conn_id);
            }
        }
        break;

    default:
        break;
    }

    // 如果有错误，发送错误事件
    if (error_code != 0)
    {
        tcp_conn_event_t evt = {.conn_id = conn->conn_id, .type = TCP_EVENT_ERROR};
        int pipefd = conn->rx_buf.pipe_fd[1];
        ssize_t rc = write(pipefd, &evt, sizeof(evt));
        if (rc != sizeof(evt))
        {
            LOG_WARN("Failed to send error event for conn[%d]\n", conn->conn_id);
        }
        else
        {
            LOG_DEBUG("Sent error event for conn[%d]\n", conn->conn_id);
        }
    }

    LOG_DEBUG("Connection [%d] state change: %s -> %s\n",
             conn->conn_id, conn_state_str(old_state), conn_state_str(new_state));
}

/**
 * @brief 关闭并清理连接资源
 * @param mgr 连接管理器
 * @param conn 要关闭的连接
 */
static void close_conn(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!conn)
        return;

    // 发送关闭事件
    update_conn_state(conn, CONN_STATE_CLOSED, 0);

    // 从epoll中移除文件描述符（如果存在）
    if (conn->io_fd >= 0)
    {
        // 从tx线程的epoll中移除（如果连接正在建立中）
        if (conn->state == CONN_STATE_CONNECTING)
        {
            struct epoll_event ev = {0};
            epoll_ctl(mgr->tx_epoll_fd, EPOLL_CTL_DEL, conn->io_fd, &ev);
        }
        // 从rx线程的epoll中移除（如果连接已建立）
        else if (conn->state == CONN_STATE_CONNECTED)
        {
            struct epoll_event ev = {0};
            epoll_ctl(mgr->rx_epoll_fd, EPOLL_CTL_DEL, conn->io_fd, &ev);
        }

        close(conn->io_fd);
        conn->io_fd = -1;
    }

    // 从连接管理器中移除连接
    pthread_mutex_lock(&mgr->lock);
    for (int i = 0; i < mgr->conn_count; i++)
    {
        if (mgr->conns[i] == conn)
        {
            // 将最后一个连接移动到当前位置，然后减少计数
            mgr->conns[i] = mgr->conns[--mgr->conn_count];
            break;
        }
    }
    pthread_mutex_unlock(&mgr->lock);

    // 清理缓冲区资源
    rx_buffer_destroy(&conn->rx_buf);
    tx_buffer_destroy(&conn->tx_buf);

    // 释放连接内存
    free(conn);
}

static void notify_rx_thread_add_fd(int epoll_fd, int fd)
{
    struct epoll_event ev = {0};
    ev.events             = EPOLLIN | EPOLLET;
    ev.data.fd            = fd;
    epoll_ctl(epoll_fd, EPOLL_CTL_ADD, fd, &ev);
    LOG_WARN("rx thread add socket fd=%d\n", fd);
}

static void notify_rx_thread_remove_fd(int epoll_fd, int fd)
{
    struct epoll_event ev = {0};
    ev.events             = EPOLLIN | EPOLLET;
    ev.data.fd            = fd;
    epoll_ctl(epoll_fd, EPOLL_CTL_DEL, fd, &ev);
    LOG_WARN("rx thread remove socket fd=%d\n", fd);
}

static void notify_tx_thread_add_fd(int epoll_fd, int fd)
{
    struct epoll_event ev = {0};
    ev.events             = EPOLLIN | EPOLLET;
    ev.data.fd            = fd;
    int rc                = epoll_ctl(epoll_fd, EPOLL_CTL_ADD, fd, &ev);
    LOG_WARN("tx thread  %d add pipe fd=%d rc=%d\n", epoll_fd, fd, rc);
}

static void notify_tx_thread_remove_fd(int epoll_fd, int fd)
{
    struct epoll_event ev = {0};
    ev.events             = EPOLLIN | EPOLLET;
    ev.data.fd            = fd;
    epoll_ctl(epoll_fd, EPOLL_CTL_DEL, fd, &ev);
    LOG_WARN("tx thread %d remove pipe fd=%d\n", epoll_fd, fd);
}

/**
 * @brief 根据文件描述符查找已连接的连接
 * @param mgr 连接管理器
 * @param fd 文件描述符
 * @return 找到的连接指针，未找到返回NULL
 */
static tcp_conn_item_t *find_conn_by_fd(tcp_conn_manage_t *mgr, int fd)
{
    tcp_conn_item_t *ret = NULL;
    pthread_mutex_lock(&mgr->lock);
    for (int i = 0; i < mgr->conn_count; i++)
    {
        tcp_conn_item_t *conn = mgr->conns[i];
        if (conn && conn->io_fd == fd && conn->state == CONN_STATE_CONNECTED)
        {
            ret = conn;
            break;
        }
    }
    pthread_mutex_unlock(&mgr->lock);
    return ret;
}

/**
 * @brief 根据文件描述符查找正在连接的连接
 * @param mgr 连接管理器
 * @param fd 文件描述符
 * @return 找到的连接指针，未找到返回NULL
 */
static tcp_conn_item_t *find_connecting_conn_by_fd(tcp_conn_manage_t *mgr, int fd)
{
    tcp_conn_item_t *ret = NULL;
    pthread_mutex_lock(&mgr->lock);
    for (int i = 0; i < mgr->conn_count; i++)
    {
        tcp_conn_item_t *conn = mgr->conns[i];
        if (conn && conn->io_fd == fd && conn->state == CONN_STATE_CONNECTING)
        {
            ret = conn;
            break;
        }
    }
    pthread_mutex_unlock(&mgr->lock);
    return ret;
}

/**
 * @brief 根据文件描述符查找监听器
 * @param mgr 连接管理器
 * @param fd 文件描述符
 * @return 找到的监听器指针，未找到返回NULL
 */
static tcp_conn_listner_t *find_listener_by_fd(tcp_conn_manage_t *mgr, int fd)
{
    ts_node_t *node = mgr->lisnter_list.head;
    while (node)
    {
        tcp_conn_listner_t *lst = (tcp_conn_listner_t *)node->data;
        if (lst && (int)(intptr_t)lst->userdata == fd)
        {
            return lst;
        }
        node = node->next;
    }
    return NULL;
}

/**
 * @brief 处理接收到的数据
 * @param mgr 连接管理器
 * @param conn 连接对象
 * @param fd 文件描述符
 */
static void handle_rx_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int fd)
{
    char    buf[4096];
    ssize_t nread;

    while ((nread = read(fd, buf, sizeof(buf))) > 0)
    {
        LOG_DEBUG("[socket] rx thread, conn=%d fd=%d nread=%d\n", conn->conn_id, fd, nread);

        // 从socket中读出的数据，写入rx_buffer后，做消息通知
        rx_buffer_write(&conn->rx_buf, buf, (size_t)nread);

        // 产生消息通知
        tcp_conn_event_t evt    = {.conn_id = conn->conn_id, .type = TCP_EVENT_RX_READY};
        int              pipefd = conn->rx_buf.pipe_fd[1];
        const ssize_t    rc     = write(pipefd, &evt, sizeof(evt));

        if (rc != sizeof(evt))
        {
            LOG_FATAL(
                "rx thread error: pipe[%d] write failed rc(%ld) error(%d): %s\n", pipefd, rc, errno, strerror(errno));
        }

        conn->last_active = time(NULL);
    }

    // 处理连接关闭或错误
    if (nread == 0 || (nread < 0 && errno != EAGAIN && errno != EWOULDBLOCK))
    {
        notify_rx_thread_remove_fd(mgr->rx_epoll_fd, conn->io_fd);
        notify_tx_thread_remove_fd(mgr->tx_epoll_fd, conn->tx_buf.pipe_fd[0]);

        // TODO: 待完善的连接重试代码
        if (conn->retry_count < conn->max_retries)
        {
            conn->retry_count++;
            LOG_WARN("Connection lost, attempting to reconnect (%d/%d)...\n", conn->retry_count, conn->max_retries);
        }

        LOG_FATAL("Max retries reached, closing connection.");
        close_conn(mgr, conn);
    }
}

static void handle_connection_established(tcp_conn_manage_t *mgr, int fd)
{
    tcp_conn_item_t *conn = find_connecting_conn_by_fd(mgr, fd);
    if (!conn)
        return;

    int       err = 0;
    socklen_t len = sizeof(err);
    getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &len);
    if (err != 0)
    {
        perror("client connect: socket  error");
        // 进一步诊断网络问题
        printf("client connect: error code: %d\n", err);

        //! 检查常见错误类型
        switch (err)
        {
        case ECONNREFUSED:
            printf("目标主机拒绝连接（服务未启动或端口错误）\n");
            break;
        case ETIMEDOUT:
            printf("连接超时（网络不通或防火墙阻挡）\n");
            break;
        case ENETUNREACH:
            printf("网络不可达（路由问题）\n");
            break;
        case EHOSTUNREACH:
            printf("主机不可达\n");
            break;
        case EADDRINUSE:
            printf("本地地址已被使用\n");
            break;
        case EINPROGRESS:
            printf("非阻塞模式连接正在进行中\n");
            break;
        default:
            printf("未知错误类型\n");
        }
        // Initiate reconnect if not exceeded max retries
        if (conn->retry_count < conn->max_retries)
        {
            conn->retry_count++;
            LOG_WARN("conn[%d] failed, retrying (%d/%d)...\n", conn->conn_id, conn->retry_count, conn->max_retries);
        }
        update_conn_state(conn, CONN_STATE_CLOSED, err);
        close_conn(mgr, conn);
        return;
    }

    update_conn_state(conn, CONN_STATE_CONNECTED, 0);
    conn->last_active = time(NULL);
    notify_rx_thread_add_fd(mgr->rx_epoll_fd, conn->io_fd);
    notify_tx_thread_add_fd(mgr->tx_epoll_fd, conn->tx_buf.pipe_fd[0]);
    LOG_WARN(
        "conn[%d] ready, rx_pipe(%d, %d) tx_pipe(%d,%d), socket %d\n",
        conn->conn_id,
        conn->rx_buf.pipe_fd[0],
        conn->rx_buf.pipe_fd[1],
        conn->tx_buf.pipe_fd[0],
        conn->tx_buf.pipe_fd[1],
        conn->io_fd);
}

static void handle_new_connection(tcp_conn_manage_t *mgr, int listen_fd)
{
    tcp_conn_listner_t *lst = find_listener_by_fd(mgr, listen_fd);
    if (!lst)
        return;

    struct sockaddr_in client_addr;
    socklen_t          len       = sizeof(client_addr);
    int                retry     = 3;
    int                socket_fd = -1;
    do
    {
        int cfd = accept(listen_fd, (struct sockaddr *)&client_addr, &len);
        if (cfd < 0)
        {
            if (errno == EAGAIN || errno == EWOULDBLOCK)
                continue;
            perror("accept");
            break;
        }
        else
        {
            socket_fd = cfd;
            set_sock_nonblock(socket_fd);
            set_sock_timeout(socket_fd, 1000 * 1000);
            break;
        }
    } while (retry-- > 0);

    if (socket_fd >= 0)
    {
        tcp_conn_item_t *conn = lst->accept_conn;
        conn->io_fd           = socket_fd;
        update_conn_state(conn, CONN_STATE_CONNECTED, 0);
        conn->last_active     = time(NULL);

        notify_rx_thread_add_fd(mgr->rx_epoll_fd, conn->io_fd);
        notify_tx_thread_add_fd(mgr->tx_epoll_fd, conn->tx_buf.pipe_fd[0]);
        LOG_WARN(
            "conn[%d] ready, rx_pipe(%d, %d) tx_pipe(%d,%d), socket %d\n",
            conn->conn_id,
            conn->rx_buf.pipe_fd[0],
            conn->rx_buf.pipe_fd[1],
            conn->tx_buf.pipe_fd[0],
            conn->tx_buf.pipe_fd[1],
            conn->io_fd);
    }
}

// ========== Socket实现 - 精细化接口 ==========

// 管理器生命周期管理
int sock_mgr_init(tcp_conn_manage_t *mgr, const tcp_conn_mgr_settings_t *settings)
{
    if (!mgr || !settings)
        return -1;

    mgr->settings    = *settings;
    mgr->is_running  = 0;
    mgr->conn_count  = 0;
    mgr->rx_epoll_fd = -1;
    mgr->tx_epoll_fd = -1;

    pthread_mutex_init(&mgr->lock, NULL);
    ts_list_init(&mgr->lisnter_list, NULL);

    return 0;
}

int sock_mgr_start(tcp_conn_manage_t *mgr)
{
    if (!mgr || mgr->is_running)
        return -1;

    mgr->rx_epoll_fd = epoll_create1(0);
    mgr->tx_epoll_fd = epoll_create1(0);
    if (mgr->rx_epoll_fd < 0 || mgr->tx_epoll_fd < 0)
    {
        perror("epoll_create1");
        return -1;
    }

    mgr->is_running = 1;
    pthread_create(&mgr->rx_tid, NULL, rx_thread_func, mgr);
    pthread_create(&mgr->tx_tid, NULL, tx_thread_func, mgr);

    return 0;
}

int sock_mgr_stop(tcp_conn_manage_t *mgr)
{
    if (!mgr || !mgr->is_running)
        return -1;

    mgr->is_running = 0;
    pthread_join(mgr->rx_tid, NULL);
    pthread_join(mgr->tx_tid, NULL);

    if (mgr->rx_epoll_fd >= 0)
        close(mgr->rx_epoll_fd);
    if (mgr->tx_epoll_fd >= 0)
        close(mgr->tx_epoll_fd);

    return 0;
}

int sock_mgr_cleanup(tcp_conn_manage_t *mgr)
{
    if (!mgr)
        return -1;

    sock_mgr_stop(mgr);

    // 清理所有连接
    pthread_mutex_lock(&mgr->lock);
    for (int i = 0; i < mgr->conn_count; i++)
    {
        if (mgr->conns[i])
        {
            close_conn(mgr, mgr->conns[i]);
            mgr->conns[i] = NULL;
        }
    }
    mgr->conn_count = 0;
    pthread_mutex_unlock(&mgr->lock);

    // 手动清理所有监听器，避免双重释放
    ts_node_t *node = mgr->lisnter_list.head;
    while (node)
    {
        tcp_conn_listner_t *lst = (tcp_conn_listner_t *)node->data;
        if (lst)
        {
            int fd = (int)(intptr_t)lst->userdata;
            if (fd >= 0)
            {
                close(fd);
                lst->userdata = (void *)(intptr_t)-1;  // 标记为已关闭
            }
            free(lst);
        }
        node = node->next;
    }

    pthread_mutex_destroy(&mgr->lock);
    ts_list_destroy(&mgr->lisnter_list);

    return 0;
}

int sock_mgr_get_manager_state(tcp_conn_manage_t *mgr)
{
    if (!mgr)
        return -1;
    return mgr->is_running ? 1 : 0;
}

// 连接建立和管理
int sock_mgr_create_connection(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_item_t **conn)
{
    if (!mgr || !conf || !conn)
        return -1;

    pthread_mutex_lock(&mgr->lock);

    tcp_conn_item_t *new_conn = (tcp_conn_item_t *)calloc(1, sizeof(tcp_conn_item_t));
    if (!new_conn)
    {
        pthread_mutex_unlock(&mgr->lock);
        return -1;
    }

    // 初始化连接对象，但不建立实际连接
    new_conn->mgr     = mgr;
    new_conn->type    = (conn_type_t)CONN_TYPE_CLIENT;
    new_conn->conn_id = mgr->conn_count;
    new_conn->state   = CONN_STATE_NONE;  // 初始状态为未连接
    new_conn->io_fd   = -1;               // 文件描述符在enable时创建

    // 复制连接信息
    strncpy(new_conn->remote_ip, conf->remote_ip, sizeof(new_conn->remote_ip) - 1);
    new_conn->remote_ip[sizeof(new_conn->remote_ip) - 1] = '\0';
    new_conn->remote_port                                = conf->remote_port;
    strncpy(new_conn->local_ip, conf->local_ip, sizeof(new_conn->local_ip) - 1);
    new_conn->local_ip[sizeof(new_conn->local_ip) - 1] = '\0';
    new_conn->local_port                               = conf->local_port;
    strncpy(new_conn->bind_interface, conf->bind_interface, sizeof(new_conn->bind_interface) - 1);
    new_conn->bind_interface[sizeof(new_conn->bind_interface) - 1] = '\0';

    new_conn->retry_count       = 0;
    new_conn->max_retries       = conf->max_retries;
    new_conn->retry_interval_ms = conf->retry_interval_ms;

    rx_buffer_init(&new_conn->rx_buf, RING_BUFFER_SIZE);
    tx_buffer_init(&new_conn->tx_buf, RING_BUFFER_SIZE);

    mgr->conns[mgr->conn_count++] = new_conn;
    pthread_mutex_unlock(&mgr->lock);

    *conn = new_conn;
    return 0;
}

int sock_mgr_create_listener(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf, tcp_conn_listner_t **listener)
{
    if (!mgr || !conf || !listener)
        return -1;

    // 创建socket但不立即bind和listen
    int fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
    if (fd < 0)
    {
        perror("socket failed");
        return -1;
    }

    int opt = 1;
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

    pthread_mutex_lock(&mgr->lock);
    tcp_conn_listner_t *lst = (tcp_conn_listner_t *)calloc(1, sizeof(tcp_conn_listner_t));
    lst->type               = CONN_TYPE_SERVER;
    lst->conn_id            = mgr->conn_count;
    lst->mgr                = mgr;
    lst->state              = CONN_STATE_NONE;  // 初始状态为未启用
    lst->userdata           = (void *)(intptr_t)fd;
    lst->accept_conn        = NULL;

    // 保存监听配置信息
    lst->local_port = conf->local_port;

    // 调试信息：打印配置中的IP地址
    printf("DEBUG: conf->local_ip = '%s' (length: %zu)\n", conf->local_ip, strlen(conf->local_ip));

    strncpy(lst->local_ip, conf->local_ip, sizeof(lst->local_ip) - 1);
    lst->local_ip[sizeof(lst->local_ip) - 1] = '\0';

    // 调试信息：打印复制后的IP地址
    printf("DEBUG: lst->local_ip = '%s' (length: %zu)\n", lst->local_ip, strlen(lst->local_ip));

    tcp_conn_item_t *conn = (tcp_conn_item_t *)calloc(1, sizeof(tcp_conn_item_t));
    conn->type            = (conn_type_t)CONN_TYPE_ACCEPT_WORKER;
    conn->conn_id         = mgr->conn_count;
    conn->mgr             = mgr;
    conn->state           = CONN_STATE_CLOSED;
    conn->listner         = lst;
    conn->io_fd           = -1;
    rx_buffer_init(&conn->rx_buf, RING_BUFFER_SIZE);
    tx_buffer_init(&conn->tx_buf, RING_BUFFER_SIZE);
    conn->last_active = time(NULL);

    mgr->conns[mgr->conn_count++] = conn;
    lst->accept_conn              = conn;
    pthread_mutex_unlock(&mgr->lock);

    ts_list_push_back(&mgr->lisnter_list, lst);

    *listener = lst;
    return 0;
}

int sock_mgr_accept_connection(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener, tcp_conn_item_t **conn)
{
    if (!mgr || !listener || !conn)
        return -1;

    *conn = listener->accept_conn;
    return (*conn != NULL) ? 0 : -1;
}

int sock_mgr_destroy_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return -1;

    close_conn(mgr, conn);
    return 0;
}

int sock_mgr_destroy_listener(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener)
{
    if (!mgr || !listener)
        return -1;

    int fd = (int)(intptr_t)listener->userdata;
    if (fd >= 0)
        close(fd);

    // 从列表中移除
    ts_list_remove(&mgr->lisnter_list, listener, NULL);

    free(listener);
    return 0;
}

// 连接控制
int sock_mgr_enable_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return -1;

    // 检查连接是否已经建立
    if (conn->state != CONN_STATE_NONE && conn->state != CONN_STATE_CLOSED)
        return 0;  // 已经启用或正在连接中

    pthread_mutex_lock(&mgr->lock);

    // 创建socket
    int fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
    if (fd < 0)
    {
        pthread_mutex_unlock(&mgr->lock);
        return -1;
    }

    set_sock_timeout(fd, 1000 * 1000);

    // 设置本地地址（如果配置了）
    if (strlen(conn->local_ip) > 0 && conn->local_port > 0)
    {
        struct sockaddr_in local_addr = {0};
        local_addr.sin_family         = AF_INET;
        local_addr.sin_port           = htons(conn->local_port);
        inet_pton(AF_INET, conn->local_ip, &local_addr.sin_addr);

        if (bind(fd, (struct sockaddr *)&local_addr, sizeof(local_addr)) < 0)
        {
            close(fd);
            pthread_mutex_unlock(&mgr->lock);
            return -1;
        }
    }

    // 设置远程地址并连接
    struct sockaddr_in addr = {0};
    addr.sin_family         = AF_INET;
    addr.sin_port           = htons(conn->remote_port);
    inet_pton(AF_INET, conn->remote_ip, &addr.sin_addr);

    int ret = connect(fd, (struct sockaddr *)&addr, sizeof(addr));
    if (ret < 0 && errno != EINPROGRESS)
    {
        close(fd);
        pthread_mutex_unlock(&mgr->lock);
        return -1;
    }

    // 更新连接信息
    conn->io_fd       = fd;
    conn->state       = CONN_STATE_CONNECTING;
    conn->last_active = time(NULL);

    pthread_mutex_unlock(&mgr->lock);

    // 添加到tx线程进行连接建立监控
    struct epoll_event ev = {0};
    ev.events             = EPOLLOUT | EPOLLET;
    ev.data.fd            = fd;
    epoll_ctl(mgr->tx_epoll_fd, EPOLL_CTL_ADD, fd, &ev);

    return 0;
}

int sock_mgr_enable_listener(tcp_conn_manage_t *mgr, tcp_conn_listner_t *listener)
{
    if (!mgr || !listener)
        return -1;

    // 调试信息：打印监听器信息
    printf("DEBUG: sock_mgr_enable_listener called\n");
    printf("DEBUG: listener->local_ip = '%s' (length: %zu)\n", listener->local_ip, strlen(listener->local_ip));
    printf("DEBUG: listener->local_port = %d\n", listener->local_port);
    printf("DEBUG: listener->state = %d\n", listener->state);

    // 检查监听器是否已经启用
    if (listener->state == CONN_STATE_LISTENING)
        return 0;  // 已经启用

    int fd = (int)(intptr_t)listener->userdata;
    if (fd >= 0)
    {
        // socket已经存在，直接使用
        struct sockaddr_in addr = {0};
        addr.sin_family         = AF_INET;
        addr.sin_port           = htons(listener->local_port);
        if (strlen(listener->local_ip) > 0)
        {
            inet_pton(AF_INET, listener->local_ip, &addr.sin_addr);
        }
        else
        {
            addr.sin_addr.s_addr = INADDR_ANY;
        }

        if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0)
        {
            perror("bind failed");
            close(fd);
            return -1;
        }

        // 开始监听
        if (listen(fd, 128) < 0)
        {
            perror("listen failed");
            close(fd);
            return -1;
        }

        // 更新监听器状态
        listener->state = CONN_STATE_LISTENING;

        // 添加到rx线程进行连接接受监控
        struct epoll_event ev = {0};
        ev.events             = EPOLLIN | EPOLLET;
        ev.data.fd            = fd;
        epoll_ctl(mgr->rx_epoll_fd, EPOLL_CTL_ADD, fd, &ev);

        printf("Listener enabled on %s:%d (fd: %d)\n", listener->local_ip, listener->local_port, fd);
    }
    else
    {
        // 创建新的socket
        fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
        if (fd < 0)
        {
            perror("socket failed");
            return -1;
        }

        int opt = 1;
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

        // 绑定地址
        struct sockaddr_in addr = {0};
        addr.sin_family         = AF_INET;
        addr.sin_port           = htons(listener->local_port);
        if (strlen(listener->local_ip) > 0)
        {
            inet_pton(AF_INET, listener->local_ip, &addr.sin_addr);
        }
        else
        {
            addr.sin_addr.s_addr = INADDR_ANY;
        }

        if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0)
        {
            perror("bind failed");
            close(fd);
            return -1;
        }

        // 开始监听
        if (listen(fd, 128) < 0)
        {
            perror("listen failed");
            close(fd);
            return -1;
        }

        // 更新监听器信息
        listener->userdata = (void *)(intptr_t)fd;
        listener->state    = CONN_STATE_LISTENING;

        // 添加到rx线程进行连接接受监控
        struct epoll_event ev = {0};
        ev.events             = EPOLLIN | EPOLLET;
        ev.data.fd            = fd;
        epoll_ctl(mgr->rx_epoll_fd, EPOLL_CTL_ADD, fd, &ev);

        printf("Listener enabled on %s:%d (fd: %d)\n", listener->local_ip, listener->local_port, fd);
    }

    return 0;
}

int sock_mgr_disable_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return -1;

    return sock_mgr_close_connection(mgr, conn);
}

int sock_mgr_reconnect(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return -1;

    // 关闭当前连接
    if (conn->io_fd >= 0)
    {
        close(conn->io_fd);
        conn->io_fd = -1;
    }

    // 重置连接状态
    conn->state       = CONN_STATE_CLOSED;
    conn->retry_count = 0;

    // 重新启用连接（这将触发重新连接）
    return sock_mgr_enable_connection(mgr, conn);
}

int sock_mgr_set_timeout(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int timeout_ms)
{
    if (!mgr || !conn || conn->io_fd < 0)
        return -1;

    return set_sock_timeout(conn->io_fd, timeout_ms);
}

int sock_mgr_set_reconnect_params(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int max_retries, int interval_ms)
{
    if (!mgr || !conn)
        return -1;

    conn->max_retries       = max_retries;
    conn->retry_interval_ms = interval_ms;
    return 0;
}

// 数据传输
int sock_mgr_send_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len)
{
    if (!mgr || !conn || !data || conn->io_fd < 0)
        return -1;

    return tx_buffer_write(&conn->tx_buf, data, len);
}

int sock_mgr_receive_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len)
{
    if (!mgr || !conn || !data)
        return -1;

    return rx_buffer_read(&conn->rx_buf, data, len);
}

int sock_mgr_send_data_nonblock(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const void *data, size_t len)
{
    return sock_mgr_send_data(mgr, conn, data, len);
}

int sock_mgr_receive_data_nonblock(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, void *data, size_t len)
{
    return sock_mgr_receive_data(mgr, conn, data, len);
}

int sock_mgr_get_available_data(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return -1;

    return rx_buffer_get_used_size(&conn->rx_buf);
}

int sock_mgr_get_available_space(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return -1;

    return tx_buffer_get_free_size(&conn->tx_buf);
}

// 状态查询
conn_state_t sock_mgr_get_connection_state(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return CONN_STATE_CLOSED;

    return conn->state;
}

int sock_mgr_get_connection_stats(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, struct tcp_conn_stats_s *stats)
{
    if (!mgr || !conn || !stats)
        return -1;

    stats->bytes_sent       = tx_buffer_get_total_sent(&conn->tx_buf);
    stats->bytes_received   = rx_buffer_get_total_received(&conn->rx_buf);
    stats->packets_sent     = 0;  // 需要在缓冲区中实现
    stats->packets_received = 0;  // 需要在缓冲区中实现
    stats->reconnect_count  = conn->retry_count;
    stats->last_active_time = conn->last_active;

    return 0;
}

int sock_mgr_get_local_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port)
{
    if (!mgr || !conn || !ip || !port || conn->io_fd < 0)
        return -1;

    struct sockaddr_in addr;
    socklen_t          len = sizeof(addr);
    if (getsockname(conn->io_fd, (struct sockaddr *)&addr, &len) < 0)
        return -1;

    inet_ntop(AF_INET, &addr.sin_addr, ip, 16);
    *port = ntohs(addr.sin_port);
    return 0;
}

int sock_mgr_get_remote_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *ip, int *port)
{
    if (!mgr || !conn || !ip || !port || conn->io_fd < 0)
        return -1;

    struct sockaddr_in addr;
    socklen_t          len = sizeof(addr);
    if (getpeername(conn->io_fd, (struct sockaddr *)&addr, &len) < 0)
        return -1;

    inet_ntop(AF_INET, &addr.sin_addr, ip, 16);
    *port = ntohs(addr.sin_port);
    return 0;
}

int sock_mgr_is_connection_active(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return 0;

    return (conn->state == CONN_STATE_CONNECTED);
}

// 连接关闭
int sock_mgr_close_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    if (!mgr || !conn)
        return -1;

    update_conn_state(conn, CONN_STATE_CLOSING, 0);
    close_conn(mgr, conn);
    return 0;
}

int sock_mgr_force_close_connection(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn)
{
    return sock_mgr_close_connection(mgr, conn);
}

int sock_mgr_close_all_connections(tcp_conn_manage_t *mgr)
{
    if (!mgr)
        return -1;

    pthread_mutex_lock(&mgr->lock);
    for (int i = 0; i < mgr->conn_count; i++)
    {
        if (mgr->conns[i])
        {
            close_conn(mgr, mgr->conns[i]);
            mgr->conns[i] = NULL;
        }
    }
    mgr->conn_count = 0;
    pthread_mutex_unlock(&mgr->lock);

    return 0;
}

// 网络配置
int sock_mgr_bind_to_interface(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *interface)
{
    if (!mgr || !conn || !interface)
        return -1;

    strncpy(conn->bind_interface, interface, sizeof(conn->bind_interface) - 1);
    conn->bind_interface[sizeof(conn->bind_interface) - 1] = '\0';
    return 0;
}

int sock_mgr_set_local_address(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, const char *ip, int port)
{
    if (!mgr || !conn)
        return -1;

    if (ip)
    {
        strncpy(conn->local_ip, ip, sizeof(conn->local_ip) - 1);
        conn->local_ip[sizeof(conn->local_ip) - 1] = '\0';
    }
    if (port > 0)
        conn->local_port = port;

    return 0;
}

int sock_mgr_set_socket_option(
    tcp_conn_manage_t *mgr,
    tcp_conn_item_t   *conn,
    int                level,
    int                optname,
    const void        *optval,
    socklen_t          optlen)
{
    if (!mgr || !conn || conn->io_fd < 0)
        return -1;

    return setsockopt(conn->io_fd, level, optname, optval, optlen);
}

int sock_mgr_get_socket_option(
    tcp_conn_manage_t *mgr,
    tcp_conn_item_t   *conn,
    int                level,
    int                optname,
    void              *optval,
    socklen_t         *optlen)
{
    if (!mgr || !conn || conn->io_fd < 0)
        return -1;

    return getsockopt(conn->io_fd, level, optname, optval, optlen);
}

// 事件处理
int sock_mgr_set_event_handler(tcp_conn_manage_t *mgr, const tcp_conn_event_handler_t *handler)
{
    if (!mgr || !handler)
        return -1;

    // TODO: 实现事件处理器设置
    return 0;
}

int sock_mgr_process_events(tcp_conn_manage_t *mgr, int timeout_ms)
{
    if (!mgr)
        return -1;

    // TODO: 实现事件处理
    return 0;
}

int sock_mgr_set_event_callback(
    tcp_conn_manage_t *mgr,
    void (*callback)(tcp_conn_event_t *event, void *userdata),
    void *userdata)
{
    if (!mgr || !callback)
        return -1;

    // TODO: 实现事件回调设置
    return 0;
}

// 缓冲区管理
int sock_mgr_set_rx_buffer_size(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size)
{
    if (!mgr || !conn)
        return -1;

    // TODO: 实现缓冲区大小设置
    return 0;
}

int sock_mgr_set_tx_buffer_size(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t size)
{
    if (!mgr || !conn)
        return -1;

    // TODO: 实现缓冲区大小设置
    return 0;
}

int sock_mgr_get_rx_buffer_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total)
{
    if (!mgr || !conn)
        return -1;

    if (used)
        *used = rx_buffer_get_used_size(&conn->rx_buf);
    if (total)
        *total = RING_BUFFER_SIZE;
    return 0;
}

int sock_mgr_get_tx_buffer_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, size_t *used, size_t *total)
{
    if (!mgr || !conn)
        return -1;

    if (used)
        *used = tx_buffer_get_used_size(&conn->tx_buf);
    if (total)
        *total = RING_BUFFER_SIZE;
    return 0;
}

// 高级功能
int sock_mgr_set_tcp_nodelay(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable)
{
    if (!mgr || !conn)
        return -1;

    int flag = enable ? 1 : 0;
    return sock_mgr_set_socket_option(mgr, conn, IPPROTO_TCP, TCP_NODELAY, &flag, sizeof(flag));
}

int sock_mgr_set_keepalive(
    tcp_conn_manage_t *mgr,
    tcp_conn_item_t   *conn,
    int                keepalive_time,
    int                keepalive_intvl,
    int                keepalive_probes)
{
    if (!mgr || !conn)
        return -1;

    int flag = 1;
    if (sock_mgr_set_socket_option(mgr, conn, SOL_SOCKET, SO_KEEPALIVE, &flag, sizeof(flag)) < 0)
        return -1;

#ifdef TCP_KEEPIDLE
    if (sock_mgr_set_socket_option(mgr, conn, IPPROTO_TCP, TCP_KEEPIDLE, &keepalive_time, sizeof(keepalive_time)) < 0)
        return -1;
#endif

#ifdef TCP_KEEPINTVL
    if (sock_mgr_set_socket_option(mgr, conn, IPPROTO_TCP, TCP_KEEPINTVL, &keepalive_intvl, sizeof(keepalive_intvl))
        < 0)
        return -1;
#endif

#ifdef TCP_KEEPCNT
    if (sock_mgr_set_socket_option(mgr, conn, IPPROTO_TCP, TCP_KEEPCNT, &keepalive_probes, sizeof(keepalive_probes))
        < 0)
        return -1;
#endif

    return 0;
}

int sock_mgr_set_reuse_addr(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable)
{
    if (!mgr || !conn)
        return -1;

    int flag = enable ? 1 : 0;
    return sock_mgr_set_socket_option(mgr, conn, SOL_SOCKET, SO_REUSEADDR, &flag, sizeof(flag));
}

int sock_mgr_set_reuse_port(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, int enable)
{
    if (!mgr || !conn)
        return -1;

    int flag = enable ? 1 : 0;
    return sock_mgr_set_socket_option(mgr, conn, SOL_SOCKET, SO_REUSEPORT, &flag, sizeof(flag));
}

// 调试和诊断
int sock_mgr_get_last_error(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *error_msg, size_t max_len)
{
    if (!mgr || !conn || !error_msg)
        return -1;

    snprintf(error_msg, max_len, "Connection state: %s", conn_state_str(conn->state));
    return 0;
}

int sock_mgr_dump_connection_info(tcp_conn_manage_t *mgr, tcp_conn_item_t *conn, char *buffer, size_t buffer_size)
{
    if (!mgr || !conn || !buffer)
        return -1;

    char local_ip[16] = {0}, remote_ip[16] = {0};
    int  local_port = 0, remote_port = 0;

    sock_mgr_get_local_address(mgr, conn, local_ip, &local_port);
    sock_mgr_get_remote_address(mgr, conn, remote_ip, &remote_port);

    snprintf(
        buffer,
        buffer_size,
        "Connection[%d] State: %s\n"
        "Local: %s:%d\n"
        "Remote: %s:%d\n"
        "Last Active: %ld\n"
        "Retry Count: %d/%d\n",
        conn->conn_id,
        conn_state_str(conn->state),
        local_ip,
        local_port,
        remote_ip,
        remote_port,
        conn->last_active,
        conn->retry_count,
        conn->max_retries);

    return 0;
}

int sock_mgr_validate_config(tcp_conn_manage_t *mgr, const struct tcp_conn_info_s *conf)
{
    if (!mgr || !conf)
        return -1;

    // 验证IP地址格式
    struct sockaddr_in addr;
    if (inet_pton(AF_INET, conf->remote_ip, &addr.sin_addr) != 1)
        return -1;

    if (strlen(conf->local_ip) > 0 && inet_pton(AF_INET, conf->local_ip, &addr.sin_addr) != 1)
        return -1;

    // 验证端口范围
    if (conf->remote_port <= 0 || conf->remote_port > 65535)
        return -1;

    if (conf->local_port < 0 || conf->local_port > 65535)
        return -1;

    return 0;
}

/**
 * @brief 接收线程函数 - 处理数据接收和连接建立
 * @param arg 连接管理器指针
 * @return NULL
 */
static void *rx_thread_func(void *arg)
{
    tcp_conn_manage_t *mgr = (tcp_conn_manage_t *)arg;
    struct epoll_event events[MAX_EVENTS];

    while (mgr->is_running)
    {
        int n = epoll_wait(mgr->rx_epoll_fd, events, MAX_EVENTS, 1000);
        for (int i = 0; i < n; i++)
        {
            int fd = events[i].data.fd;

            // 检查是否是监听socket
            tcp_conn_listner_t *lst = find_listener_by_fd(mgr, fd);
            if (lst)
            {
                if (events[i].events & EPOLLIN)
                {
                    handle_new_connection(mgr, fd);
                }
                continue;
            }

            // 处理数据接收
            tcp_conn_item_t *conn = find_conn_by_fd(mgr, fd);
            if (!conn)
                continue;

            if (events[i].events & EPOLLIN)
            {
                handle_rx_data(mgr, conn, fd);
            }
        }
    }
    return NULL;
}

static void *tx_thread_func(void *arg)
{
    tcp_conn_manage_t *mgr = (tcp_conn_manage_t *)arg;
    struct epoll_event events[MAX_EVENTS];
    while (mgr->is_running)
    {
        int n = epoll_wait(mgr->tx_epoll_fd, events, MAX_EVENTS, 100);
        for (int i = 0; i < n; i++)
        {
            int fd = events[i].data.fd;

            tcp_conn_item_t *connecting_conn = find_connecting_conn_by_fd(mgr, fd);
            if (connecting_conn && (events[i].events & EPOLLOUT))
            {
                handle_connection_established(mgr, fd);
                // Remove from tx thread since connection is established
                struct epoll_event ev = {0};
                epoll_ctl(mgr->tx_epoll_fd, EPOLL_CTL_DEL, fd, &ev);
                continue;
            }

            // Handle pipe events for data transmission
            // 使用 EPOLLET（边沿触发），必须 drain pipe 中所有事件，避免第二条消息被卡住
            tcp_conn_event_t evt;
            ssize_t          rc;
            while ((rc = read(fd, &evt, sizeof(evt))) == (ssize_t)sizeof(evt))
            {
                if (evt.type != TCP_EVENT_TX_READY)
                    continue;

                tcp_conn_item_t *conn = mgr->conns[evt.conn_id];
                if (!conn)
                    continue;

                const void *data;
                size_t      len;
                //! 从 tx_buffer 中取走一块, tx_buffer 不同与 rx_buffer, 非连续, 增加4字节用来分段
                if (tx_buffer_peek_next(&conn->tx_buf, &data, &len) == 0 && len > 0)
                {
                    ssize_t nsent = write(conn->io_fd, data, len);
                    // dump_bytes_array(data, len, 0);
                    LOG_DEBUG("[socket] tx thread, conn=%d fd=%d nsent=%d\n", conn->conn_id, conn->io_fd, nsent);
                    if (nsent > 0)
                    {
                        tx_buffer_consume(&conn->tx_buf, (size_t)nsent);
                        conn->last_active = time(NULL);
                    }
                    else if (nsent < 0 && errno != EAGAIN && errno != EWOULDBLOCK)
                    {
                        LOG_FATAL("write failed. close socket.");
                        close_conn(mgr, conn);
                        break;
                    }
                    // TODO: 进一步完善异步处理, 发送失败, 需要做异步通知, 进行连接管理
                }
            }
        }
    }
    return NULL;
}
