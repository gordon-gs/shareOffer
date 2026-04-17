# TCP连接配置文件加载测试工具

## 概述

这个工具用于加载TCP连接配置文件并格式化打印配置内容。它可以直接从源码编译，支持JSON格式的配置文件。

## 功能特性

- 加载JSON格式的TCP连接配置文件
- 格式化打印配置内容，包括：
  - 版本信息和描述
  - 全局设置
  - ARP表信息
  - 连接配置详情
- 支持多种配置类型（socket、instanta、toe）
- 错误处理和参数验证

## 编译

在 `host/tcp_conn_lib/tools` 目录下运行：

```bash
make
```

这将编译生成 `config_loader_test` 可执行文件。

## 使用方法

### 基本用法

```bash
./config_loader_test <配置文件路径>
```

### 示例

```bash
./config_loader_test ../tpl_of_tcp_conn_config.json

```

### 输出示例

```
正在加载配置文件: ../tpl_of_tcp_conn_config.json
配置文件加载成功！

================ TCP CONFIGURATION ================
 Version      : 1.0
 Description  : TCP Connection and Buffer Management
 Type         : socket
---------------------------------------------------
 Global Settings:
   Max Connections   : 16
   Ring Buffer Size  : 8192 bytes
   Reconnect Interval: 0
   Connection Timeout: 0
---------------------------------------------------
 ARP Table (4 entries):
   [00] IP: 192.168.56.1     MAC: c0:e7:2e:fa:44:9d  Local: Yes
   [01] IP: 192.168.58.1     MAC: ca:8b:38:e2:70:22  Local: Yes
   [02] IP: 192.168.56.11    MAC: 74:3e:39:00:06:92  Local: No
   [03] IP: 192.168.58.11    MAC: 74:3e:39:00:06:93  Local: No
---------------------------------------------------
 Connections (2 total):
 [00] tcp_server_1800
      Type       : Server
      Enabled    : YES
      State      : None
      Local      : 192.168.56.1   :18000 ()
      Remote     : 192.168.56.1   :0     ()
      RX Buffer  : 0 / 8192 bytes
      TX Buffer  : 0 / 8192 bytes
---------------------------------------------------
 [01] tcp_client_192_
      Type       : Client
      Enabled    : YES
      State      : None
      Local      : 192.168.56.1   :0     ()
      Remote     : 192.168.56.11  :15201 ()
      RX Buffer  : 0 / 8192 bytes
      TX Buffer  : 0 / 8192 bytes
---------------------------------------------------
================ END OF CONFIG ====================

配置类型 'socket' 支持的操作接口: 可用
配置文件加载和打印完成。
```

## 配置文件格式

工具支持JSON格式的配置文件，主要包含以下部分：

```json
{
    "version": "1.0",
    "description": "TCP Connection Configuration",
    "type": "socket",
    "global_settings": {
        "max_connections": 16,
        "ring_buffer_size": 8192,
        "reconnect_interval": 5000,
        "connection_timeout": 10000
    },
    "arp_table": [
        {
            "host_mac": "00:00:00:00:00:00",
            "host_ip": "127.0.0.1",
            "is_local": true
        }
    ],
    "connections": [
        {
            "conn_id": 0,
            "conn_tag": "tcp_server_12345",
            "conn_type": "server",
            "local_ip": "127.0.0.1",
            "local_port": 12345,
            "remote_ip": "127.0.0.1",
            "remote_port": 0
        },
        {
            "conn_id": 1,
            "conn_tag": "tcp_client_12345",
            "conn_type": "client",
            "local_ip": "127.0.0.1",
            "local_port": 0,
            "remote_ip": "127.0.0.1",
            "remote_port": 12345
        }
    ]
}
```

## Makefile 命令

- `make` - 编译测试工具
- `make clean` - 清理编译文件
- `make rebuild` - 重新编译
- `make test` - 编译并运行测试
- `make install` - 安装到系统（需要sudo权限）

## 依赖

- libtcp_conn.a - TCP连接库
- libjson-c.a - JSON解析库
- 系统库：libpthread, libm

## 错误处理

工具提供基本的错误处理：
- 参数数量不正确时显示使用说明
- 配置文件不存在时显示错误信息
- 配置文件格式错误时显示解析错误

## 注意事项

1. 确保配置文件存在且可读
2. 配置文件必须是有效的JSON格式
3. 工具需要在包含依赖库的环境中运行
4. 支持的配置类型：socket、instanta、toe
