#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <uv.h>
#include <pthread.h>
#include <unistd.h>

#define MAGIC_NUMBER        0x11114444
#define RETURN_MAGIC_NUMBER 0x44441111

#define BUFFER_SIZE     8192
#define IDLE_TIMEOUT_MS 20000
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
    int                active;
} tcp_server_t;

uv_loop_t   *loop;
tcp_server_t servers[MAX_SERVERS];
int          server_ports[MAX_SERVERS] = {7000, 7001, 7002, 7003};

client_t  *client_list_head = NULL;
client_t  *client_list_tail = NULL;
uv_mutex_t client_list_mutex;
uv_mutex_t server_mutex;
uv_async_t server_control_async;

typedef enum
{
    ADD_SERVER,
    REMOVE_SERVER
} server_command_type;

typedef struct
{
    server_command_type type;
    int                 port;
} server_command_t;

server_command_t pending_command;

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
                client_t *tmp    = client_list_head;
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
    server_ref->active             = 1;
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

void stop_server(int port)
{
    for (int i = 0; i < MAX_SERVERS; ++i)
    {
        if (servers[i].active && servers[i].port == port)
        {
            uv_close((uv_handle_t *)&servers[i].server_handle, NULL);
            servers[i].active = 0;
            printf("Stopped server on port %d\n", port);
            return;
        }
    }
    printf("No active server found on port %d\n", port);
}

void server_control_cb(uv_async_t *handle)
{
    uv_mutex_lock(&server_mutex);
    if (pending_command.type == ADD_SERVER)
    {
        for (int i = 0; i < MAX_SERVERS; ++i)
        {
            if (!servers[i].active)
            {
                start_server(pending_command.port, &servers[i]);
                break;
            }
        }
    }
    else if (pending_command.type == REMOVE_SERVER)
    {
        stop_server(pending_command.port);
    }
    uv_mutex_unlock(&server_mutex);
}

void add_server_async(int port)
{
    uv_mutex_lock(&server_mutex);
    pending_command.type = ADD_SERVER;
    pending_command.port = port;
    uv_mutex_unlock(&server_mutex);
    uv_async_send(&server_control_async);
}

void remove_server_async(int port)
{
    uv_mutex_lock(&server_mutex);
    pending_command.type = REMOVE_SERVER;
    pending_command.port = port;
    uv_mutex_unlock(&server_mutex);
    uv_async_send(&server_control_async);
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

void *start_uv_loop(void *arg)
{
    printf("[LoopThread] Running libuv loop\n");
    uv_run(loop, UV_RUN_DEFAULT);
    return NULL;
}

int main(int argc, char **argv)
{
    loop = uv_default_loop();
    uv_mutex_init(&client_list_mutex);
    uv_mutex_init(&server_mutex);
    uv_async_init(loop, &server_control_async, server_control_cb);

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
        start_server(server_ports[i], &servers[i]);
    }

    pthread_t loop_thread;
    pthread_create(&loop_thread, NULL, start_uv_loop, NULL);

    sleep(3);
    add_server_async(8000);
    sleep(3);
    remove_server_async(8000);

    pthread_join(loop_thread, NULL);

    close_all_clients();
    uv_mutex_destroy(&client_list_mutex);
    uv_mutex_destroy(&server_mutex);

    return 0;
}
