# TCP 连接库单元测试

## 测试执行方法

### 构建所有测试程序
```bash
make all
```

### 运行测试

#### 使用第一个配置文件运行所有测试
```bash
make test
```

#### 使用第二个配置文件运行所有测试
```bash
make test-config2
```

#### 使用特定配置文件运行测试
```bash
make test-tcp_conn_config
make test-tcp_conn_loopback_config
```

#### 单独运行特定测试
```bash
make test-send      # 发送测试
make test-server    # 服务器事件测试
make test-client    # 客户端事件测试
make test-iperf     # iperf 转发测试
```

#### 手动运行测试（支持自定义配置文件）
```bash
# 使用默认配置文件
./build/tcp_conn_send_utest_main
./build/tcp_client_event_utest_main
./build/tcp_server_event_utest_main
./build/tcp_iperf_forward_utest_main

# 使用指定配置文件
./build/tcp_conn_send_utest_main tcp_conn_config.json
./build/tcp_client_event_utest_main tcp_conn_loopback_config.json
./build/tcp_server_event_utest_main custom_config.json
./build/tcp_iperf_forward_utest_main custom_config.json
```

### 清理构建文件
```bash
make clean
```

### 获取帮助信息
```bash
make help
```

## 环境变量设置

### 设置日志级别
可以通过设置 `INSTANTA_LOG_LEVEL` 环境变量来控制日志输出级别：

```bash
# 设置日志级别为 5 并运行测试
INSTANTA_LOG_LEVEL=5 make test

# 设置日志级别为 5 并手动运行测试
INSTANTA_LOG_LEVEL=5 ./build/tcp_conn_send_utest_main tcp_conn_config.json
INSTANTA_LOG_LEVEL=5 ./build/tcp_client_event_utest_main tcp_conn_loopback_config.json
```

## 配置文件

- `tcp_conn_config.json` - 默认配置文件
- `tcp_conn_loopback_config.json` - 回环配置文件

## 测试用例

- `tcp_client_event_utest_main.c` - 客户端事件测试
- `tcp_conn_send_utest_main.c` - 发送测试
- `tcp_iperf_forward_utest_main.c` - iperf 转发测试
- `tcp_server_event_utest_main.c` - 服务器事件测试

## 功能特性

- **自动识别测试用例**：Makefile 自动识别 `*_utest_main.c` 文件作为测试用例
- **支持多个配置文件**：Makefile 自动识别 `*.json` 文件作为配置文件
- **命令行参数支持**：所有测试程序都支持通过命令行参数指定配置文件
- **动态测试目标**：为每个配置文件自动生成对应的测试目标
- **环境变量控制**：支持通过环境变量设置日志级别
