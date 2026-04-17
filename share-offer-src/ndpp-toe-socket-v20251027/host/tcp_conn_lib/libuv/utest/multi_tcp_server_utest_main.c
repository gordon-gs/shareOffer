#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <uv.h>

#define MAGIC_NUMBER        0x11114444
#define RETURN_MAGIC_NUMBER 0x44441111

#define BUFFER_SIZE     8192
#define IDLE_TIMEOUT_MS 2000
#define MAX_SERVERS     4

typedef struct client_s
{
    uv_tcp_t         handle;
    uv_timer_t       idle_timer;
    uv_buf_t         read_buffer;
    struct client_s *next;
} client_t;

typedef struct
{
    uv_tcp_t           server_handle;
    struct sockaddr_in addr;
    int                port;
} tcp_server_t;

uv_loop_t   *loop;
tcp_server_t servers[MAX_SERVERS];
int          server_ports[MAX_SERVERS] = {7000, 7001, 7002, 7003};

// 连接链表头尾指针
client_t *client_list_head = NULL;
client_t *client_list_tail = NULL;
// 保护链表的互斥锁
uv_mutex_t client_list_mutex;

void add_client(client_t *client)
{
    uv_mutex_lock(&client_list_mutex);
    client->next = NULL;
    if (client_list_tail)
    {
        client_list_tail->next = client;
        client_list_tail       = client;
    }
    else
    {
        client_list_head = client_list_tail = client;
    }
    uv_mutex_unlock(&client_list_mutex);
}

void remove_client(client_t *client)
{
    uv_mutex_lock(&client_list_mutex);
    client_t **curr = &client_list_head;
    while (*curr)
    {
        if (*curr == client)
        {
            *curr = client->next;
            if (client == client_list_tail)
            {
                client_list_tail = NULL;
                // 如果还有节点，则更新尾指针
                client_t *tmp = client_list_head;
                while (tmp && tmp->next)
                {
                    tmp = tmp->next;
                }
                client_list_tail = tmp;
            }
            break;
        }
        curr = &(*curr)->next;
    }
    uv_mutex_unlock(&client_list_mutex);
}

void close_client(client_t *client)
{
    printf("Closing client connection\n");

    uv_timer_stop(&client->idle_timer);
    uv_close((uv_handle_t *)&client->handle, NULL);
    uv_close((uv_handle_t *)&client->idle_timer, NULL);

    remove_client(client);

    if (client->read_buffer.base)
    {
        free(client->read_buffer.base);
    }
    free(client);
}

void on_idle_timeout(uv_timer_t *timer)
{
    client_t *client = (client_t *)timer->data;
    printf("Client idle timeout, closing connection.\n");
    close_client(client);
}

void on_write_end(uv_write_t *req, int status)
{
    if (status)
    {
        fprintf(stderr, "Write error: %s\n", uv_strerror(status));
    }
    free(req->data);
    free(req);
}

void alloc_buffer(uv_handle_t *handle, size_t suggested_size, uv_buf_t *buf)
{
    buf->base = (char *)malloc(suggested_size);
    buf->len  = suggested_size;
}

void echo_read(uv_stream_t *stream, ssize_t nread, const uv_buf_t *buf)
{
    client_t *client = (client_t *)stream->data;

    if (nread > 0)
    {
        // 重置超时定时器
        uv_timer_stop(&client->idle_timer);
        uv_timer_start(&client->idle_timer, on_idle_timeout, IDLE_TIMEOUT_MS, 0);

        if (nread >= 8)
        {
            uint32_t magic, pkt_id;
            memcpy(&magic, buf->base, 4);
            memcpy(&pkt_id, buf->base + 4, 4);
            magic  = ntohl(magic);
            pkt_id = ntohl(pkt_id);

            if (magic == MAGIC_NUMBER)
            {
                // 构造响应包
                uint32_t resp_magic  = htonl(RETURN_MAGIC_NUMBER);
                uint32_t resp_pkt_id = htonl(pkt_id);

                size_t payload_len = nread - 8;
                size_t resp_len    = 8 + payload_len;

                char *resp = (char *)malloc(resp_len);
                memcpy(resp, &resp_magic, 4);
                memcpy(resp + 4, &resp_pkt_id, 4);
                memset(resp + 8, 0xA5, payload_len);

                uv_buf_t wrbuf = uv_buf_init(resp, (unsigned int)resp_len);

                uv_write_t *write_req = (uv_write_t *)malloc(sizeof(uv_write_t));
                write_req->data       = resp;

                uv_write(write_req, stream, &wrbuf, 1, on_write_end);

                printf("Sent reply to client: ID=%u, Length=%zu\n", pkt_id, resp_len);
            }
            else
            {
                printf("Invalid magic number received: 0x%x\n", magic);
            }
        }
        else
        {
            // 非协议包简单应答
            const char *msg       = "unrecognized message";
            uv_buf_t    wrbuf     = uv_buf_init((char *)msg, (unsigned int)strlen(msg));
            uv_write_t *write_req = (uv_write_t *)malloc(sizeof(uv_write_t));
            write_req->data       = NULL;
            uv_write(write_req, stream, &wrbuf, 1, on_write_end);
        }
    }
    else if (nread < 0)
    {
        if (nread != UV_EOF)
            fprintf(stderr, "Read error %s\n", uv_err_name(nread));
        close_client(client);
    }

    if (buf->base)
        free(buf->base);
}

void on_new_connection(uv_stream_t *server, int status)
{
    if (status < 0)
    {
        fprintf(stderr, "New connection error: %s\n", uv_strerror(status));
        return;
    }

    client_t *client = (client_t *)malloc(sizeof(client_t));
    uv_tcp_init(loop, &client->handle);
    client->handle.data = client;

    if (uv_accept(server, (uv_stream_t *)&client->handle) == 0)
    {
        client->read_buffer = uv_buf_init(NULL, 0);
        uv_timer_init(loop, &client->idle_timer);
        client->idle_timer.data = client;
        uv_timer_start(&client->idle_timer, on_idle_timeout, IDLE_TIMEOUT_MS, 0);

        add_client(client);

        uv_read_start((uv_stream_t *)&client->handle, alloc_buffer, echo_read);

        struct sockaddr_storage addr;
        int                     len = sizeof(addr);
        uv_tcp_getpeername(&client->handle, (struct sockaddr *)&addr, &len);
        char ip[17] = {0};
        uv_ip4_name((struct sockaddr_in *)&addr, ip, 16);
        printf("New client connected from %s\n", ip);
    }
    else
    {
        uv_close((uv_handle_t *)&client->handle, NULL);
        free(client);
    }
}

int start_server(int port, tcp_server_t *server_ref)
{
    uv_tcp_init(loop, &server_ref->server_handle);
    uv_ip4_addr("0.0.0.0", port, &server_ref->addr);

    int r = uv_tcp_bind(&server_ref->server_handle, (const struct sockaddr *)&server_ref->addr, 0);
    if (r)
    {
        fprintf(stderr, "Bind error on port %d: %s\n", port, uv_strerror(r));
        return r;
    }

    server_ref->port               = port;
    server_ref->server_handle.data = server_ref;

    r = uv_listen((uv_stream_t *)&server_ref->server_handle, 128, on_new_connection);
    if (r)
    {
        fprintf(stderr, "Listen error on port %d: %s\n", port, uv_strerror(r));
        return r;
    }

    printf("Listening on port %d\n", port);
    return 0;
}

void close_all_clients()
{
    uv_mutex_lock(&client_list_mutex);
    client_t *cur = client_list_head;
    while (cur)
    {
        uv_close((uv_handle_t *)&cur->handle, NULL);
        uv_close((uv_handle_t *)&cur->idle_timer, NULL);
        cur = cur->next;
    }
    client_list_head = client_list_tail = NULL;
    uv_mutex_unlock(&client_list_mutex);
}

int main(int argc, char **argv)
{
    loop = uv_default_loop();
    uv_mutex_init(&client_list_mutex);

    int listen_count = MAX_SERVERS;
    if (argc > 1)
    {
        listen_count = argc - 1;
        if (listen_count > MAX_SERVERS)
            listen_count = MAX_SERVERS;
        for (int i = 0; i < listen_count; i++)
        {
            server_ports[i] = atoi(argv[i + 1]);
        }
    }

    for (int i = 0; i < listen_count; i++)
    {
        if (start_server(server_ports[i], &servers[i]) != 0)
        {
            fprintf(stderr, "Failed to start server on port %d\n", server_ports[i]);
            return 1;
        }
    }

    printf("Server started. Press Ctrl+C to quit.\n");

    uv_run(loop, UV_RUN_DEFAULT);

    close_all_clients();
    uv_mutex_destroy(&client_list_mutex);

    return 0;
}
