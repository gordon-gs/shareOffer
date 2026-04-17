#!/bin/bash
# 批量启动 moc_server 的脚本
# 监听地址: 127.0.0.1
# 端口范围: 18009-18017

# 可执行文件路径
EXECUTABLE="./moc_server"

# 检查可执行文件是否存在
if [ ! -f "$EXECUTABLE" ]; then
    echo "错误: moc_server 可执行文件不存在，请先编译 (cargo build )"
    exit 1
fi

# 启动多个实例
for port in {18009..18017}; do
    echo "启动 moc_server，监听端口: $port"
    # 在后台运行，并将输出重定向到日志文件
    "$EXECUTABLE" -H 127.0.0.1 -p "$port" &> "moc_server_$port.log" &
done

echo "所有 moc_server 实例已启动"
echo "使用以下命令查看进程: ps aux | grep moc_server"
echo "日志文件: server_logs/moc_server_1800x.log"
echo "使用以下命令停止所有 moc_server 实例: pkill -f moc_server"