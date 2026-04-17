#!/bin/bash

BASE_PORT=18000
CLIENT_COUNT=9

# 检查或创建 tmux 会话
if ! tmux has-session -t shareoffer 2>/dev/null; then
    tmux new-session -d -s shareoffer
fi

# 在现有会话中创建新窗口
for ((i=0; i<$CLIENT_COUNT; i++))
do
    PORT=$((BASE_PORT + i))
    echo "启动客户端连接端口 $PORT"
    tmux new-window -t shareoffer: -n "client_$PORT" "./moc_client -H 127.0.0.1 -P $PORT -A"
done

# 附加到会话以便查看
tmux attach-session -t shareoffer