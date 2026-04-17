# 编译器配置
CC = gcc
CFLAGS = -std=gnu99 -w -O2 -g

# 如果需要使用 g++，取消注释以下两行
# CC = g++
# CFLAGS = -std=c++11 -w -O2 -g

# 复杂应用开发使用高版本 gcc， 直接源码交付应当检查
# CC = /opt/rh/devtoolset-10/root/usr/bin/g++
# CFLAGS = -std=c++17 -w -O2 -g

# 包含路径
CFLAGS += -I/usr/local/include -I..

# 静态库
LIBS += -L.. -l:libtcp_conn.a

# LIBS += -l:libinstanta.a -ldl
LIBS += -pthread

# 自动获取所有测试源文件 (*.c)
TEST_SRCS = $(wildcard *_utest_main.c)
TEST_OBJS = $(patsubst %.c, build/%.o, $(TEST_SRCS))
TEST_TARGETS = $(patsubst %.c, build/%, $(TEST_SRCS))

# 获取配置文件
CONFIG_FILES = $(wildcard *.json)
CONFIG_NAMES = $(basename $(CONFIG_FILES))

# 默认目标：构建所有测试程序
all: $(TEST_TARGETS)

# 生成目标文件的通用规则
build/%.o: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -c $< -o $@

# 生成可执行文件的通用规则
build/%: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -o $@ $< $(LIBS)

# 为每个测试程序创建构建目标
$(TEST_TARGETS): build/%: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -o $@ $< $(LIBS)

# 构建所有测试程序
build-all: $(TEST_TARGETS)

# 测试目标：使用第一个配置文件运行所有测试
test: $(TEST_TARGETS)
	@for target in $(TEST_TARGETS); do \
		echo "Running $$target with config $(firstword $(CONFIG_FILES))..."; \
		./$$target $(firstword $(CONFIG_FILES)); \
	done

# 测试目标：使用第二个配置文件运行所有测试
test-config2: $(TEST_TARGETS)
	@for target in $(TEST_TARGETS); do \
		if [ -f "$(word 2,$(CONFIG_FILES))" ]; then \
			echo "Running $$target with config $(word 2,$(CONFIG_FILES))..."; \
			./$$target $(word 2,$(CONFIG_FILES)); \
		else \
			echo "Second config file not found, skipping."; \
		fi; \
	done

# 为每个配置文件创建测试目标
define create_test_target
test-$(1): $(TEST_TARGETS)
	@for target in $(TEST_TARGETS); do \
		echo "Running $$$$target with config $(1).json..."; \
		./$$$$target $(1).json; \
	done
endef

# 为每个配置文件生成测试目标
$(foreach config,$(CONFIG_NAMES),$(eval $(call create_test_target,$(config))))

# 兼容旧的测试目标
test-send: build/tcp_conn_send_utest_main
	./build/tcp_conn_send_utest_main

test-server: build/tcp_server_event_utest_main
	./build/tcp_server_event_utest_main

test-client: build/tcp_client_event_utest_main
	./build/tcp_client_event_utest_main

test-iperf: build/tcp_iperf_forward_utest_main
	./build/tcp_iperf_forward_utest_main

# 帮助信息
help:
	@echo "TCP 连接库单元测试 Makefile 使用说明"
	@echo "======================================"
	@echo ""
	@echo "构建目标:"
	@echo "  make all          - 构建所有测试程序"
	@echo "  make clean        - 清理构建文件"
	@echo ""
	@echo "测试目标:"
	@echo "  make test         - 使用第一个配置文件运行所有测试"
	@echo "  make test-config2 - 使用第二个配置文件运行所有测试"
	@echo "  make test-tcp_conn_config        - 使用 tcp_conn_config.json 运行测试"
	@echo "  make test-tcp_conn_loopback_config - 使用 tcp_conn_loopback_config.json 运行测试"
	@echo ""
	@echo "单独测试目标:"
	@echo "  make test-send    - 运行发送测试"
	@echo "  make test-server  - 运行服务器事件测试"
	@echo "  make test-client  - 运行客户端事件测试"
	@echo "  make test-iperf   - 运行 iperf 转发测试"
	@echo ""
	@echo "环境变量设置:"
	@echo "  设置日志级别: INSTANTA_LOG_LEVEL=5"
	@echo "  使用示例: INSTANTA_LOG_LEVEL=5 make test"
	@echo "           INSTANTA_LOG_LEVEL=5 ./build/tcp_conn_send_utest_main tcp_conn_config.json"
	@echo ""
	@echo "手动运行测试:"
	@echo "  ./build/tcp_conn_send_utest_main [config_file]"
	@echo "  ./build/tcp_client_event_utest_main [config_file]"
	@echo "  ./build/tcp_server_event_utest_main [config_file]"
	@echo "  ./build/tcp_iperf_forward_utest_main [config_file]"

# 清理
clean:
	rm -rf build
cc:
	rm -rf build

# 伪目标
.PHONY: all test test-event test-server-event test-client-event clean event server-event client-event event-all help
