#!/usr/bin/env python3
# encoding: utf-8

import socket
import threading

#! sudo systemctl stop firewalld.service
HOST = "0.0.0.0"   # 监听所有网卡
PORT = 12345       # 监听端口
MAX_TCP_PAYLOAD_SIZE = 7500 #发生粘包时,可能产生的最大包长度

def handle_client(conn, addr):
    """处理单个客户端连接的函数"""
    print(f"Thread {threading.current_thread().name} handling client {addr}")

    with conn:
        while True:
            data = conn.recv(MAX_TCP_PAYLOAD_SIZE)
            if not data:
                print(f"Client {addr} disconnected from thread {threading.current_thread().name}")
                break
            print(f"Received from {addr} in thread {threading.current_thread().name}, length {len(data)} : {data.decode(errors='ignore')}")
            conn.sendall(data)  # 原样返回

def run_echo_server():
    # 创建 TCP socket
    server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)  # 端口复用
    server_sock.bind((HOST, PORT))
    server_sock.listen(5)  # 最大等待队列

    print(f"Echo server listening on {HOST}:{PORT}")

    while True:
        conn, addr = server_sock.accept()
        print(f"New connection from {addr}")

        # 为每个客户端创建独立的工作线程轮询收到的数据
        client_thread = threading.Thread(target=handle_client, args=(conn, addr), daemon=True)
        client_thread.start()
        print(f"Started thread {client_thread.name} for client {addr}")

if __name__ == "__main__":
    run_echo_server()
