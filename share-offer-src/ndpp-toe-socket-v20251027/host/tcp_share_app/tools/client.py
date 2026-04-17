import socket
import time
import threading
import sys
import struct

MAGIC_NUMBER = 0x11114444

# 包大小列表
PACKET_SIZES = [64, 128, 256, 512, 1024, 1500]  # 可根据需要修改
PACKET_SWITCH_INTERVAL = 1000  # 每发送N个包切换一次PACKET_SIZE
SEND_INTERVAL = 3  # 秒

packet_id = 1
packet_id_lock = threading.Lock()

packet_count = 0
packet_count_lock = threading.Lock()

packet_size_index = 0


def get_current_packet_size():
    """获取当前应使用的 PACKET_SIZE"""
    global packet_count, packet_size_index
    with packet_count_lock:
        packet_count += 1
        if packet_count % PACKET_SWITCH_INTERVAL == 0:
            packet_size_index = (packet_size_index + 1) % len(PACKET_SIZES)
        return PACKET_SIZES[packet_size_index] - 56


def build_packet():
    """构造一个格式化的数据包"""
    global packet_id

    with packet_id_lock:
        pid = packet_id
        packet_id += 1

    current_size = get_current_packet_size()

    # 构建头部: magic(4) + id(4)
    header = struct.pack(">I I", MAGIC_NUMBER, pid)
    padding_length = max(0, current_size - len(header))
    payload = header + bytes([0x5A] * padding_length)
    return payload, pid, current_size


def send_messages(client_socket):
    """定时发送格式化消息"""
    try:
        while True:
            packet, pid, size = build_packet()
            client_socket.sendall(packet)
            print(f"Sent packet: id={pid}, len={size}")
            time.sleep(SEND_INTERVAL)
    except (BrokenPipeError, ConnectionResetError) as e:
        print(f"Error in sending messages: {e}. Exiting send thread.")
    except Exception as e:
        print(f"Unexpected error in send thread: {e}")
    finally:
        print("Sender thread is closing.")


def parse_packet(data):
    """解析接收到的数据包"""
    if len(data) < 8:
        print(f"Incomplete packet received. Length: {len(data)}")
        return

    magic, pid = struct.unpack(">I I", data[:8])
    print(f"Received Packet -> Magic: {magic:08X}, ID: {pid}, Length: {len(data)}")


def receive_messages(client_socket):
    """接收服务器消息并解析打印"""
    try:
        while True:
            data = client_socket.recv(8192)
            if data:
                parse_packet(data)
            else:
                print("Server closed the connection.")
                break
    except ConnectionResetError:
        print("Connection lost. Exiting receiver thread.")
    except Exception as e:
        print(f"Unexpected error in receive thread: {e}")
    finally:
        print("Receiver thread is closing.")

    
def tcp_client(server_ip, server_port):
    client_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)

    # 设置超时时间为 10 秒
    client_socket.settimeout(20.0)
    
    server_address = (server_ip, server_port)
    print(f"Connecting to {server_address}...")

    try:
        client_socket.connect(server_address)

        sender_thread = threading.Thread(target=send_messages, args=(client_socket,))
        sender_thread.daemon = True
        sender_thread.start()

        receiver_thread = threading.Thread(target=receive_messages, args=(client_socket,))
        receiver_thread.daemon = True
        receiver_thread.start()

        while sender_thread.is_alive() and receiver_thread.is_alive():
            time.sleep(1)

    except ConnectionRefusedError:
        print("Failed to connect to the server. Is the server running?")
    finally:
        print("Closing connection")
        client_socket.close()


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python client.py <server_ip> <server_port>")
    else:
        server_ip = sys.argv[1]
        server_port = int(sys.argv[2])
        tcp_client(server_ip, server_port)
