# 编译器设置
CC = gcc
CFLAGS = -std=gnu99 -Wall -O2 -g
CFLAGS += -I.. -I../tcp_conn -I../applog -I../json-c/include

# 源文件与目标
SRCS = config_loader_test.c
TARGET = config_loader_test

# 依赖的静态库
LIBTCP_CONN = ../libtcp_conn.a
LIBJSON_C = ../json-c/lib64/libjson-c.a

# 系统库
SYSLIBS = -lpthread -lm

# 默认目标
all: $(TARGET)

# 编译测试工具
$(TARGET): $(SRCS) $(LIBTCP_CONN)
	$(CC) $(CFLAGS) -o $@ $(SRCS) $(LIBTCP_CONN) $(LIBJSON_C) $(SYSLIBS)
	@echo "编译完成: $(TARGET)"

# 确保依赖库存在
$(LIBTCP_CONN):
	@echo "正在编译依赖库 libtcp_conn.a..."
	$(MAKE) -C ../tcp_conn

# 清理
clean:
	rm -f $(TARGET)
	@echo "清理完成"

# 重新编译
rebuild: clean all

# 测试
test: $(TARGET)
	@echo "运行测试..."
	./$(TARGET) ../tpl_of_tcp_conn_config.json

# 安装（可选）
install: $(TARGET)
	@echo "安装 $(TARGET) 到 /usr/local/bin..."
	sudo cp $(TARGET) /usr/local/bin/
	@echo "安装完成"

.PHONY: all clean rebuild test install
