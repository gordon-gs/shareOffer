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
CFLAGS += -I/usr/local/include -I../../libuv/include

# 静态库
LIBS += -L../../libuv/lib -l:libuv.a

# 可选
# LIBS += -l:libinstanta.a -ldl 

LIBS += -pthread -ldl 

# 源文件
SRCS = add_tcp_server_utest_main.c

# 目标文件（在 build 目录中生成）
OBJS = $(patsubst %, build/%, $(SRCS:.c=.o))

# 最终目标
TARGET = build/utest_libuv

# 默认目标
all: $(TARGET)

# 生成目标文件
$(OBJS): $(SRCS)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -c $< -o $@

# 生成可执行文件
$(TARGET): $(OBJS) 
	$(CC) $(CFLAGS) -o $@ $(OBJS) $(LIBS)

# 测试
test: $(TARGET)
	# ./$(TARGET)
	env INSTANTA_LOG_LEVEL=3 taskset -c 4 ./$(TARGET)

# 清理
clean:
	rm -rf build

# 伪目标
.PHONY: all test clean
