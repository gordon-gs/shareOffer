#!/usr/bin/expect -f

# --- 配置 ---
set timeout 10      ;# 设置全局超时时间（秒）。如果程序在10秒内没有任何响应，脚本会超时退出。
set client_path "./moc_client" ;# moc_client 可执行文件的路径。如果不在当前目录，请使用绝对路径。

# --- 脚本开始 ---
# 启动 moc_client 程序
spawn $client_path

# 等待程序启动并显示出第一个提示符 ">"
# "expect" 命令会一直等待，直到接收到 ">" 或者超时
expect ">"

# --- 发送第一个命令: logon ---
send "logon\r"      ;# 发送 "logon" 命令，\r 表示回车
puts "--- 已发送 'logon' 命令 ---"
expect ">"          ;# 等待命令执行完毕后，程序再次显示提示符 ">"

# 等待1秒，确保登录成功
sleep 1

# --- 2. 发送开启心跳命令: heartbeat ---
send "heartbeat\r"
puts "--- 已发送 'heartbeat' 命令 ---"
expect ">" ;# 等待命令执行完毕

# --- 发送第二个命令: order ---
send "order\r"
puts "--- 已发送 'order' 命令 ---"
expect ">"

sleep 3

# --- 发送第三个命令: order1 ---
send "order_userinfo_is_null\r"
puts "--- 已发送 'order_userinfo_is_null' 命令 ---"
expect ">"

sleep 3

# --- logout---
send "logout\r"
puts "--- 已发送 'logout' 命令 ---"
expect ">"

# --- 发送退出命令 ---
send "quit\r"
puts "--- 已发送 'quit' 命令，脚本结束 ---"

# 等待程序完全退出
expect eof
