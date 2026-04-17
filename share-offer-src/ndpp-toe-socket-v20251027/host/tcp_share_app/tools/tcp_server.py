import socket
import threading
import struct
import sys

MAGIC_NUMBER = 0x11114444
RETURN_MAGIC_NUMBER = 0x44441111

def handle_client_connection(client_socket, client_address):
    """处理客户端连接的函数，响应来自单个客户端的格式化请求"""
    print(f"New connection from {client_address}")
    try:
        while True:
            data = client_socket.recv(8192)
            if not data:
                print(f"Connection with {client_address} closed.")
                break

            print(f"Received from {client_address}: {len(data)} bytes")

            # 校验长度是否至少为格式化头部
            if len(data) >= 8:
                try:
                    magic, pkt_id = struct.unpack(">I I", data[:8])
                    if magic != MAGIC_NUMBER:
                        print(f"Invalid magic number from {client_address}")
                        continue  # 忽略非协议包

                    # 构造响应包：替换方向字段为 DIRECTION_REPLY
                    new_header = struct.pack(">I I", RETURN_MAGIC_NUMBER, pkt_id)
                    padding_length = max(0, len(data)  - len(new_header))
                    payload_rest = bytes([0xA5] * padding_length)
                    response_packet = new_header + payload_rest

                    client_socket.sendall(response_packet)
                    print(f"Sent to {client_address}: ID={pkt_id}, Length={len(response_packet)}")

                except struct.error as e:
                    print(f"Failed to parse packet from {client_address}: {e}")
            else:
                # 非协议包简单应答
                response = "unrecognized message"
                client_socket.sendall(response.encode())
                print(f"Sent to {client_address}: {response}")
    except Exception as e:
        print(f"Error handling connection with {client_address}: {e}")
    finally:
        client_socket.close()

def tcp_server(server_ip, server_port):
    # 创建TCP/IP socket
    server_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server_socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    
    # 绑定服务器地址和端口
    server_address = (server_ip, server_port)
    server_socket.bind(server_address)

    # 监听传入的连接
    server_socket.listen(5)  # 同时最多处理5个连接
    print(f"Server is listening on {server_address}...")

    IDLE_TIMEOUT = 2000
    try:
        while True:
            # 等待新连接
            client_socket, client_address = server_socket.accept()
            client_socket.settimeout(IDLE_TIMEOUT)

            # 禁用 Nagle 算法，确保数据立即发送
            client_socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            
            # TCP_SYNCNT = 7, 设置为最多1次重试
            # client_socket.setsockopt(socket.IPPROTO_TCP, 7, 1)  

            # 为每个客户端启动一个新的线程来处理连接
            client_thread = threading.Thread(target=handle_client_connection, args=(client_socket, client_address))
            client_thread.daemon = True  # 确保主线程退出时子线程自动结束
            client_thread.start()

    except KeyboardInterrupt:
        print("Server is shutting down.")
    except socket.timeout:
        print(f"{client_socket} idle for {IDLE_TIMEOUT}s, disconnecting.")
    finally:
        server_socket.close()

# 启动TCP服务器
if __name__ == "__main__":
    # 默认的服务器 IP 和端口
    DEFAULT_SERVER_IP = "0.0.0.0"
    DEFAULT_SERVER_PORT = 15201
    
    # 解析命令行参数
    if len(sys.argv) < 3:
        print(f"Usage: python tcp_server.py <local_host> <local_port>")
        print(f"Using default values: {DEFAULT_SERVER_IP}:{DEFAULT_SERVER_PORT}")
        server_ip = DEFAULT_SERVER_IP
        server_port = DEFAULT_SERVER_PORT
    else:
        server_ip = sys.argv[1]
        server_port = int(sys.argv[2])
    
    tcp_server(server_ip, server_port)  


