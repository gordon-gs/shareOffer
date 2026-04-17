#!/bin/bash

# 批量MOC客户端启动脚本
SERVER_IP="127.0.0.1"
START_PORT=18000
END_PORT=18008
CLIENT_COUNT=10  # 要启动的客户端数量
LOG_DIR="./client_logs"  # 日志目录

# 创建日志目录
mkdir -p "$LOG_DIR"

# 启动多个客户端
for ((i=0; i<CLIENT_COUNT; i++)); do
    PORT=$((START_PORT + i))
    LOG_FILE="$LOG_DIR/client_$PORT.log"
    
    # 使用nohup在后台运行客户端程序
    nohup ./moc_client \
        -H "$SERVER_IP" \
        -P "$PORT" \
        -O 48 \
        -S 49 \
        -T 49 \
        -C 111 \
        > "$LOG_FILE" 2>&1 &
    
    # 获取进程ID
    PID=$!
    
    # 等待客户端启动并发送命令
    sleep 1
    
    # 发送logon命令和heartbeat命令
    echo "logon" >> "$LOG_FILE"
    echo "heartbeat" >> "$LOG_FILE"
    
    echo "已启动客户端 $((i+1))/$CLIENT_COUNT, 端口: $PORT, PID: $PID"
done

echo "所有客户端已启动，日志保存在 $LOG_DIR 目录下"