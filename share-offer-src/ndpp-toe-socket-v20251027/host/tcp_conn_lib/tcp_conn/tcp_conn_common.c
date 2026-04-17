#include "tcp_conn_common.h"
#include "tcp_conn_socket.h"
#include "tcp_conn_instanta.h"
#include "tcp_conn_toe.h"
// #include "ndpp/ndpp_toe.h"
// #include "yusur_ndpp/yusur_ndpp.h"
#include "json-c/json.h"

const char *tcp_conn_lib_version(void)
{
    return TCP_CONN_VERSION_TEXT;
}

const char *tcp_conn_strerror(int err)
{
    switch (err)
    {
#ifdef NDPP_NDPP_H
    case NDPP_EINVAL:
        return "Invalid arguments";
    case NDPP_EAGAIN:
        return "Resource temporarily unavailable";
    case NDPP_EMSGSIZE:
        return "Transmit message size error";
    case NDPP_ENOFEEDBACK:
        return "No transmit feedback";
    case NDPP_ENODEV:
        return "No such NDPP device";
    case NDPP_EREGOFFSET:
        return "Invalid register offset";
    case NDPP_EBUFINUSE:
        return "Buffer already in use";
    case NDPP_ETRUNCATED:
        return "Received message would be truncated";
    case NDPP_EHUGEPAGES:
        return "Hugepages not available";
    case NDPP_ELICENSE:
        return "Invalid license";
    case NDPP_EDEVTYPE:
        return "Not NDPP device";
    case NDPP_ENOTSUP:
        return "Operation not supported";
#endif
#ifdef _USER_NDPP_TOE_H_
        //! 扩充至被TOE使用的错误码
    case TOE_ERR_NO_MEM:
        return "TOE: Memory allocation failed";
    case TOE_ERR_NO_RESOURCE:
        return "TOE: No such channel, or already in use";
    case TOE_ERR_NOT_READY:
        return "TOE: Socket not in correct state";
    case TOE_ERR_IO:
        return "TOE: I/O operation failed";
    case TOE_ERR_TIMEOUT:
        return "TOE: Operation timed out";
    case TOE_ERR_ALREADY:
        return "TOE: Operation already in progress";
#endif
    default:
        return strerror(err);  //! socket 标准错误码
    }
}

static tcp_conn_manage_t *g_mgr = NULL;

/**
 * @brief 发送连接状态变化事件
 * @param conn 连接对象
 * @param event_type 事件类型
 * @return 成功返回0，失败返回-1
 */
static int send_conn_event(tcp_conn_item_t *conn, conn_event_type_t event_type)
{
    if (!conn)
        return -1;

    tcp_conn_event_t evt = {.conn_id = conn->conn_id, .type = event_type};

    int     pipefd = conn->rx_buf.pipe_fd[1];
    ssize_t rc     = write(pipefd, &evt, sizeof(evt));

    if (rc != sizeof(evt))
    {
        LOG_WARN("Failed to send event %d for conn[%d]: %s\n", event_type, conn->conn_id, strerror(errno));
        return -1;
    }

    LOG_DEBUG("Sent event %d for conn[%d]\n", event_type, conn->conn_id);
    return 0;
}

/**
 * @brief 处理连接状态变化，发送相应事件
 * @param conn 连接对象
 * @param old_state 旧状态
 * @param new_state 新状态
 * @param error_code 错误码（如果有错误）
 */
static void
    handle_conn_state_change(tcp_conn_item_t *conn, conn_state_t old_state, conn_state_t new_state, int error_code)
{
    if (!conn)
        return;

    LOG_DEBUG(
        "Connection [%d] state change: %s -> %s\n",
        conn->conn_id,
        conn_state_str(old_state),
        conn_state_str(new_state));

    // 根据状态变化发送相应事件
    switch (new_state)
    {
    case CONN_STATE_CONNECTED:
        send_conn_event(conn, TCP_EVENT_CONNECTED);
        break;

    case CONN_STATE_CLOSING:
        send_conn_event(conn, TCP_EVENT_CLOSING);
        break;

    case CONN_STATE_CLOSED:
        send_conn_event(conn, TCP_EVENT_CLOSED);
        break;

    default:
        break;
    }

    // 如果有错误，发送错误事件
    if (error_code != 0)
    {
        send_conn_event(conn, TCP_EVENT_ERROR);
    }
}

tcp_conn_manage_t *tcp_conn_mgr_create(const char *path)
{
    if (g_mgr)
    {
        LOG_WARN("Only one TCP connection manager instance is supported.\n");
        return g_mgr;
    }

    applog_init(APPLOG_DEBUG, 0);
    tcp_conn_manage_t *mgr = (tcp_conn_manage_t *)calloc(1, sizeof(*mgr));
    assert(mgr);
    g_mgr = mgr;
    pthread_mutex_init(&mgr->lock, NULL);
    ts_list_init(&mgr->lisnter_list, NULL);

    //! 析配置文件
    load_tcp_config(path, &mgr->settings);
#if 0
    show_tcp_config(&mgr->settings);
#endif

    //! 根据配置文件类型选择对应的操作接口
    mgr->ops = get_tcp_conn_ops_by_type(mgr->settings.type);
    if (!mgr->ops)
    {
        LOG_ERROR("Failed to get TCP connection ops for type: %s\n", mgr->settings.type);
        free(mgr);
        return NULL;
    }

    //! 设置管理器类型
    if (strcmp(mgr->settings.type, "socket") == 0 || strcmp(mgr->settings.type, "instanta") == 0)
    {
        mgr->type = TCP_TYPE_INSTANTA;
    }
    else if (strcmp(mgr->settings.type, "toe") == 0)
    {
        mgr->type = TCP_TYPE_NDPP_TOE;
    }
    else
    {
        mgr->type = TCP_TYPE_INSTANTA;  // 默认类型
    }

    LOG_INFO("TCP manager created with type: %s\n", mgr->settings.type);

    //! 总是在后台加载2个线程,分别用于数据的接收和发送
    tcp_conn_mgr_load_settings(mgr);

    mgr->ops->start(mgr);

    return mgr;
}

int tcp_conn_mgr_load_settings(tcp_conn_manage_t *mgr)
{
    mgr->max_conns = mgr->settings.conn_count;
    ts_list_init(&mgr->conn_cfg_list_, NULL);

    LOG_INFO("Loading %d connections from settings\n", mgr->max_conns);

    for (int i = 0; i < mgr->max_conns; i++)
    {
        tcp_conn_info_t *cfg = mgr->settings.conn_list + i;
        LOG_INFO("Processing connection [%d]: %s (%s)\n", cfg->conn_id, cfg->conn_tag, conn_type_str(cfg->conn_type));

        if (CONN_TYPE_CLIENT == cfg->conn_type)
        {
            tcp_client_config_t *client = (tcp_client_config_t *)calloc(1, sizeof(tcp_client_config_t));
            strcpy(client->remote_ip, cfg->remote_ip);
            client->remote_port = cfg->remote_port;

            ts_list_push_back(&mgr->conn_cfg_list_, client);

            tcp_conn_item_t *conn   = NULL;
            int              result = mgr->ops->create_connection(mgr, cfg, &conn);
            if (result == 0 && conn)
            {
                LOG_INFO("Created client connection [%d] successfully\n", cfg->conn_id);
            }
            else
            {
                LOG_ERROR("Failed to create client connection [%d], result: %d\n", cfg->conn_id, result);
            }
        }
        if (CONN_TYPE_SERVER == cfg->conn_type)
        {
            tcp_server_config_t *server = (tcp_server_config_t *)calloc(1, sizeof(tcp_server_config_t));

            server->listen_port = cfg->local_port;
            strcpy(server->listen_ip, cfg->local_ip);
            server->max_clients                = 1;
            server->num_configs                = 1;
            server->client_configs.remote_port = 0;
            strcpy(server->client_configs.remote_ip, cfg->remote_ip);

            ts_list_push_back(&mgr->conn_cfg_list_, server);

            tcp_conn_listner_t *listener = NULL;
            int                 result   = mgr->ops->create_listener(mgr, cfg, &listener);
            if (result == 0 && listener)
            {
                LOG_INFO("Created server listener [%d] successfully\n", cfg->conn_id);
                // 设置连接对象与监听器的关联
                if (listener->accept_conn)
                {
                    listener->accept_conn->listner = listener;
                }
            }
            else
            {
                LOG_ERROR("Failed to create server listener [%d], result: %d\n", cfg->conn_id, result);
            }
        }
        cfg->stats = (tcp_conn_stats_t *)calloc(1, sizeof(tcp_conn_stats_t));
        assert(cfg->stats);
        memset(cfg->stats, 0, sizeof(tcp_conn_stats_t));
    }
    return 0;
}

void tcp_conn_mgr_destroy(tcp_conn_manage_t *mgr)
{
    if (mgr)
        mgr->ops->cleanup(mgr);
    applog_shutdown();
}

// 获取连接的配置信息
const tcp_conn_info_t *tcp_conn_get_info(tcp_conn_item_t *tcp_conn)
{
    if (tcp_conn && tcp_conn->mgr)
        return tcp_conn->mgr->settings.conn_list + tcp_conn->conn_id;
    return NULL;
}

tcp_conn_item_t *tcp_conn_find_by_id(tcp_conn_manage_t *tcp_mgr, uint16_t conn_id)
{
    if (!tcp_mgr || conn_id < 0 || conn_id >= tcp_mgr->max_conns)
        return NULL;
    return tcp_mgr->conns[conn_id];
}

int tcp_conn_state(tcp_conn_item_t *conn)
{
    if (conn)
        return conn->state;
    RETURN_ERROR(EINVAL);
}

int tcp_conn_get_event_fd(tcp_conn_item_t *conn)
{
    if (conn)
        return rx_buffer_fd(&conn->rx_buf);
    RETURN_ERROR(EINVAL);
}

int tcp_conn_connect(tcp_conn_item_t *conn)
{
    if (conn && conn->mgr && conn->mgr->ops && conn->mgr->ops->enable_connection)
        return conn->mgr->ops->enable_connection(conn->mgr, conn);
    RETURN_ERROR(EINVAL);
}

int tcp_conn_listen(tcp_conn_item_t *conn)
{
    if (conn && conn->mgr && conn->mgr->ops && conn->mgr->ops->enable_listener && conn->listner)
        return conn->mgr->ops->enable_listener(conn->mgr, conn->listner);
    RETURN_ERROR(EINVAL);
}

int tcp_conn_send(tcp_conn_item_t *conn, const void *data, int len)
{
    if (!conn)
        RETURN_ERROR(EINVAL);

    //! 发送数据 1/2 步骤: 直接更新 tx_buffer
    int n = tx_buffer_write(&conn->tx_buf, data, len);
    LOG_DEBUG("tcp_conn_send: conn[%d] write tx buffer %d bytes done", conn->conn_id, len);
    //! 发送数据 2/2 步骤: tx pipe 更新通知
    tcp_conn_event_t evt    = {.conn_id = conn->conn_id, .type = TCP_EVENT_TX_READY};
    int              pipefd = conn->tx_buf.pipe_fd[1];
    const ssize_t    rc     = write(pipefd, &evt, sizeof(evt));
    if (rc != sizeof(evt))
    {
        LOG_ERROR(
            "tcp_conn_send: conn[%d] write tx pipe[%d] failed rc(%ld) error(%d):%s\n",
            conn->conn_id,
            pipefd,
            rc,
            errno,
            strerror(errno));
    }
    LOG_DEBUG("tcp_conn_send: conn[%d] write tx pipe[%d] done", conn->conn_id, pipefd);
    return n;
}

int tcp_conn_recv(tcp_conn_item_t *conn, const void **data, int *len)
{
    if (!conn || !data || !len)
        RETURN_ERROR(EINVAL);

    //! 读数据, 其实就是从缓冲区拿, 尽可能多拿, 不拷贝数据, 返回数据位置和数据长度

    size_t size = 0;
    int    rc   = rx_buffer_peek(&conn->rx_buf, data, &size);

    *len = (int)size;
    return rc;
}

int tcp_conn_consume(tcp_conn_item_t *conn, int len)
{
    if (!conn || (len < 0))
        RETURN_ERROR(EINVAL);
    //! 配合 tcp_conn_recv 操作, 可以拆分为多个步骤, 将连续的大段数据, 分段标记,返还
    rx_buffer_consume(&conn->rx_buf, len);
    return 0;
}

int tcp_conn_close(tcp_conn_item_t *conn)
{
    if (conn && conn->mgr && conn->mgr->ops && conn->mgr->ops->close_connection)
    {
        return conn->mgr->ops->close_connection(conn->mgr, conn);
    }
    RETURN_ERROR(EINVAL);
}

int tcp_conn_reset(tcp_conn_item_t *conn)
{
    if (conn && conn->mgr && conn->mgr->ops && conn->mgr->ops->force_close_connection)
    {
        return conn->mgr->ops->force_close_connection(conn->mgr, conn);
    }
    RETURN_ERROR(EINVAL);
}
