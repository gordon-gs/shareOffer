# 编译器配置
CC = gcc
CFLAGS = -std=gnu99 -w -O2 -g

# 包含路径
CFLAGS += -I/usr/local/include -I..

# 静态库
LIBS += -L.. -l:libtcp_conn.a
LIBS += -pthread

# 示例程序源文件
EXAMPLE_SRCS = tcp_client_main.c tcp_server_main.c
EXAMPLE_TARGETS = $(patsubst %.c, build/%, $(EXAMPLE_SRCS))

# 获取配置文件
CONFIG_FILES = $(wildcard *.json)

# 默认目标：构建所有示例程序
all: $(EXAMPLE_TARGETS)

# 生成可执行文件的通用规则
build/%: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -o $@ $< $(LIBS)

# 为每个示例程序创建构建目标
$(EXAMPLE_TARGETS): build/%: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -o $@ $< $(LIBS)

# 运行客户端示例
run-client: build/tcp_client_main
	@echo "Running TCP client example..."
	./build/tcp_client_main $(firstword $(CONFIG_FILES))

# 运行服务器示例
run-server: build/tcp_server_main
	@echo "Running TCP server example..."
	./build/tcp_server_main $(firstword $(CONFIG_FILES))

# 清理
clean:
	rm -rf build

# 伪目标
.PHONY: all run-client run-server clean
