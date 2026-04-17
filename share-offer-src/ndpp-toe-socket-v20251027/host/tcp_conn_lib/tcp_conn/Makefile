# 编译器设置
CC = gcc

CFLAGS = -fPIC -std=gnu99 -Wall -O2 -g -I.. -I../applog -I../json-c/include

# 如果需要使用 g++，取消注释以下两行
# CC = g++
# CFLAGS = -std=c++11 -w -O2 -g

# 复杂应用开发使用高版本 gcc， 直接源码交付应当检查
# CC = /opt/rh/devtoolset-10/root/usr/bin/g++
# CFLAGS = -std=c++17 -w -O2 -g

#临时关闭编译告警(调试用)
CFLAGS += -w -Wno-all
# 工具设置
AR = ar
ARFLAGS = rcs
MKDIR = mkdir -p
RM = rm -rf

# 源文件与路径
SRCS = ringbuff.c \
		tcp_buffer_rx_tx.c \
		tcp_conn_common.c \
		tcp_conn_misc.c \
		tcp_conn_instanta.c \
		tcp_conn_socket.c \
		tcp_conn_toe.c \
		tcp_conn_config.c \
		ts_list.c \
		../applog/applog.c

# 中间目录与目标
BUILD_DIR = build
TEMP_DIR = $(BUILD_DIR)/temp
TARGET = ../libtcp_conn.a

# 外部静态库（支持多个）
EXTERNAL_LIBS := ../json-c/lib64/libjson-c.a
EXTERNAL_LIBS := $(abspath $(EXTERNAL_LIBS))  # 转为绝对路径

# 构造 .o 文件路径（将所有中间文件放到 build/ 中）
OBJS = $(patsubst %.c,$(BUILD_DIR)/%.o,$(notdir $(SRCS)))

# 默认目标
all: $(TARGET)

# 编译源文件到 build/
$(BUILD_DIR)/%.o: %.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/%.o: ../applog/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# 创建 build/ 临时目录
$(BUILD_DIR):
	$(MKDIR) $(BUILD_DIR)

$(TEMP_DIR):
	$(MKDIR) $(TEMP_DIR)

# 构建目标静态库（解压多个外部库并打包）
$(TARGET): $(OBJS) | $(TEMP_DIR)
	# 清空临时目录
	$(RM) $(TEMP_DIR)/*
	# 清空临时目录
	@for lib in $(EXTERNAL_LIBS); do \
		(cd $(TEMP_DIR) && $(AR) x $${lib}); \
	done
	$(AR) $(ARFLAGS) $@ $(OBJS) $(TEMP_DIR)/*.o

# 清理
clean:
	$(RM) $(BUILD_DIR) $(TARGET)

cc: clean $(TARGET)

.PHONY: all clean cc
