# 编译器配置
# CC = gcc
# CFLAGS = -std=gnu99 -w -O2 -g

# 如果需要使用 g++，取消注释以下两行
CC = g++
CFLAGS = -std=c++11 -w -O2 -g

# 复杂应用开发使用高版本 gcc， 直接源码交付应当检查
# CC = /opt/rh/devtoolset-10/root/usr/bin/g++
# CFLAGS = -std=c++17 -w -O2 -g

# 包含路径
CFLAGS += -I/usr/local/include -I../tcp_conn_lib

# 静态库
LIBS += -L../tcp_conn_lib -l:libtcp_conn.a

# LIBS += -l:libinstanta.a -ldl
LIBS += -pthread

# 源文件
SRCS +=

# 目标文件（在 build 目录中生成）
OBJS = $(patsubst %, build/%, $(SRCS:.c=.o))

# 最终目标
TARGET = build/tcp_share_app

# 默认目标
all: $(TARGET)

# 生成目标文件
$(OBJS): build/%.o: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -c $< -o $@

# 生成可执行文件
$(TARGET): $(OBJS) build/tcp_share_forward_app_main.o
	$(CC) $(CFLAGS) -o $@ build/tcp_share_forward_app_main.o $(OBJS) $(LIBS)

# 生成主程序的目标文件
build/tcp_share_forward_app_main.o: tcp_share_forward_app_main.c
	@mkdir -p build
	$(CC) $(CFLAGS) -c $< -o $@

# 测试
test: $(TARGET)
	# ./$(TARGET) tcp_share_config.json
	env INSTANTA_LOG_LEVEL=5 ./$(TARGET) ./tcp_share_config.json

# 清理
clean:
	rm -rf build

# 伪目标
.PHONY: all test clean