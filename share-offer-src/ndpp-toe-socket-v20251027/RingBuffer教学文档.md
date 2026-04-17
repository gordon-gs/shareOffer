# RingBuffer (环形缓冲区) 教学文档

## 目录
1. [什么是环形缓冲区](#什么是环形缓冲区)
2. [数据结构设计](#数据结构设计)
3. [核心概念](#核心概念)
4. [函数详解](#函数详解)
5. [实际应用场景](#实际应用场景)
6. [常见问题](#常见问题)

---

## 什么是环形缓冲区

环形缓冲区（Ring Buffer）是一种特殊的先进先出（FIFO）数据结构，它使用一块固定大小的连续内存空间，通过头尾指针的循环移动实现数据的连续读写。

**为什么使用环形缓冲区？**
- ✅ **避免内存碎片**：固定大小的连续内存，无需动态分配
- ✅ **高效率**：O(1) 时间复杂度的读写操作
- ✅ **零拷贝**：可以直接返回缓冲区内存指针，避免数据拷贝
- ✅ **线程安全**：配合原子操作实现无锁或低锁竞争

**可视化示例**：

```
初始状态 (空缓冲区):
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │   │   │   │   │   │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
    ↑
   head=0, tail=0

写入 3 个字节 "ABC":
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ A │ B │ C │   │   │   │   │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
    ↑           ↑
  tail=0      head=3

读取 2 个字节 (AB):
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ A │ B │ C │   │   │   │   │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
            ↑   ↑
          tail=2 head=3

写入 6 个字节 "DEFGHI" (环绕):
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ H │ I │ C │ D │ E │ F │ G │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
        ↑   ↑
    head=2 tail=2  (注意: head 从位置3写到了位置1，发生了环绕)
```

---

## 数据结构设计

### ringbuff_t 结构体定义

```c
typedef struct
{
    uint8_t        *data;    // 指向实际数据缓冲区的指针
    size_t          size;    // 缓冲区总大小
    volatile size_t head;    // 写入位置 (生产者指针)
    volatile size_t tail;    // 读取位置 (消费者指针)
    int             full;    // 缓冲区满标志 (区分满和空的情况)
    char            _pad[64 - (sizeof(size_t) * 2 + sizeof(int))];  // 缓存行填充
} ringbuff_t;
```

### 字段详解

| 字段 | 类型 | 说明 |
|------|------|------|
| **data** | uint8_t* | 实际存储数据的内存区域，在 init 时动态分配 |
| **size** | size_t | 缓冲区总容量（字节） |
| **head** | volatile size_t | 写指针，指示下一个写入位置 |
| **tail** | volatile size_t | 读指针，指示下一个读取位置 |
| **full** | int | 满标志，解决 head==tail 的二义性问题 |
| **_pad** | char[] | 缓存行填充，避免伪共享（False Sharing） |

### 为什么需要 full 标志？

在环形缓冲区中，`head == tail` 有两种情况：
1. **缓冲区为空**：还没写入任何数据
2. **缓冲区满**：写满了整个缓冲区

使用 `full` 标志可以明确区分这两种状态。

### 为什么使用缓存行填充？

```c
char _pad[64 - (sizeof(size_t) * 2 + sizeof(int))];
```

这是一种性能优化技术，称为**避免伪共享（False Sharing）**：

- CPU 缓存以**缓存行**（通常 64 字节）为单位加载数据
- 如果多个线程修改同一缓存行的不同变量，会导致缓存失效
- 通过填充使结构体大小对齐到 64 字节，确保不同实例不会共享缓存行

---

## 核心概念

### 1. 原子操作与内存序

本项目使用 GCC 内置的原子操作函数确保多线程安全：

```c
// 原子读取 (消费者读取生产者的指针)
size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);

// 原子写入 (生产者更新自己的指针)
__atomic_store_n(&rb->head, new_head, __ATOMIC_RELEASE);
```

**内存序说明**：
- `__ATOMIC_ACQUIRE`：用于读操作，确保后续操作不会被重排到该读取之前
- `__ATOMIC_RELEASE`：用于写操作，确保之前的操作不会被重排到该写入之后
- `__ATOMIC_RELAXED`：无内存序保证，仅保证原子性

### 2. 数据长度计算

根据 head 和 tail 的相对位置，计算当前数据量：

```c
// 情况1: head >= tail (未发生环绕)
//   ┌───┬───┬───┬───┬───┐
//   │   │ D │ D │ D │   │
//   └───┴───┴───┴───┴───┘
//       ↑           ↑
//     tail=1      head=4
// data_len = head - tail = 4 - 1 = 3

// 情况2: head < tail (发生环绕)
//   ┌───┬───┬───┬───┬───┐
//   │ D │ D │   │   │ D │
//   └───┴───┴───┴───┴───┘
//       ↑       ↑       ↑
//     head=2  tail=4  size=5
// data_len = size - tail + head = 5 - 4 + 2 = 3
```

### 3. 环绕处理

写入或读取时，如果到达缓冲区末尾，需要环绕到开头：

```c
// 取模运算实现环绕
head = (head + len) % rb->size;
```

---

## 函数详解

### 1. ringbuff_init - 初始化环形缓冲区

**函数签名**：
```c
int ringbuff_init(ringbuff_t *rb, size_t size);
```

**功能**：创建并初始化一个环形缓冲区

**代码详解**：
```c
int ringbuff_init(ringbuff_t *rb, size_t size)
{
    // 步骤1: 参数校验
    if (!rb || size < 2)
        return -1;

    // 步骤2: 分配内存
    rb->data = (uint8_t *)malloc(size);
    if (!rb->data)
        return -1;

    // 步骤3: 初始化字段
    rb->size = size;
    __atomic_store_n(&rb->head, 0, __ATOMIC_RELAXED);  // 写指针归零
    __atomic_store_n(&rb->tail, 0, __ATOMIC_RELAXED);  // 读指针归零
    rb->full = 0;                                       // 初始为空
    memset(rb->_pad, 0, sizeof(rb->_pad));             // 填充区清零

    return 0;  // 成功
}
```

**执行流程**：
```
输入: rb (未初始化的结构体), size=8192

第1步: 检查参数有效性
  ✓ rb 不为空
  ✓ size >= 2

第2步: 分配 8192 字节内存
  ┌─────────────────────────────────┐
  │  8192 字节连续内存空间           │
  └─────────────────────────────────┘
   ↑
  rb->data 指向这里

第3步: 初始化状态
  rb->size = 8192
  rb->head = 0
  rb->tail = 0
  rb->full = 0

结果: 返回 0 (成功)
```

**使用示例**：
```c
ringbuff_t my_buffer;
if (ringbuff_init(&my_buffer, 8192) == 0) {
    printf("缓冲区初始化成功，大小: 8192 字节\n");
}
```

---

### 2. ringbuff_write - 写入数据

**函数签名**：
```c
int ringbuff_write(ringbuff_t *rb, const void *src, size_t len);
```

**功能**：向环形缓冲区写入数据

**代码详解**：
```c
int ringbuff_write(ringbuff_t *rb, const void *src, size_t len)
{
    // 步骤1: 参数校验
    if (!rb || !src || len == 0)
        return 0;

    // 步骤2: 读取当前指针位置
    size_t head = __atomic_load_n(&rb->head, __ATOMIC_RELAXED);  // 我的写指针
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_ACQUIRE);  // 对方的读指针

    // 步骤3: 计算剩余空间
    size_t free_space;
    if (rb->full)
    {
        free_space = 0;  // 缓冲区满，无空间
    }
    else if (tail > head)
    {
        // 情况1: tail 在 head 右边 (中间是空闲空间)
        //   ┌───┬───┬───┬───┬───┐
        //   │ D │ D │   │   │ D │
        //   └───┴───┴───┴───┴───┘
        //       ↑       ↑
        //     head=2  tail=4
        free_space = tail - head;
    }
    else
    {
        // 情况2: tail 在 head 左边或相等 (空间在两端)
        //   ┌───┬───┬───┬───┬───┐
        //   │   │   │ D │ D │   │
        //   └───┴───┴───┴───┴───┘
        //   ↑           ↑       ↑
        // tail=0      head=4  size=5
        free_space = rb->size - head + tail;
    }

    // 步骤4: 调整写入长度（如果空间不足）
    if (len > free_space)
        len = free_space;  // 只写入能容纳的部分

    if (len == 0)
        return 0;  // 无空间可写

    // 步骤5: 处理环绕写入
    size_t first_part = rb->size - head;  // 从 head 到缓冲区末尾的空间
    if (first_part > len)
        first_part = len;  // 如果不需要环绕，只写 len 字节

    // 写入第一部分 (从 head 开始)
    memcpy(rb->data + head, src, first_part);
    
    // 写入第二部分 (从缓冲区开头开始，如果需要环绕)
    memcpy(rb->data, (const uint8_t *)src + first_part, len - first_part);

    // 步骤6: 更新 head 指针
    head = (head + len) % rb->size;  // 取模实现环绕
    __atomic_store_n(&rb->head, head, __ATOMIC_RELEASE);

    // 步骤7: 更新 full 标志
    rb->full = (head == tail) ? 1 : 0;

    return (int)len;  // 返回实际写入的字节数
}
```

**可视化执行流程**：

**场景1：简单写入（无环绕）**
```
初始状态:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │   │   │   │   │   │   │  size=8
   └───┴───┴───┴───┴───┴───┴───┴───┘
    ↑
   head=0, tail=0

调用: ringbuff_write(rb, "ABC", 3)

计算空闲空间:
  free_space = size - head + tail = 8 - 0 + 0 = 8
  len = 3 (不需要调整)

写入过程:
  first_part = min(8-0, 3) = 3
  memcpy(data+0, "ABC", 3)  // 写入 ABC
  memcpy(data+0, ..., 0)    // 无第二部分

更新指针:
  head = (0 + 3) % 8 = 3

结果:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ A │ B │ C │   │   │   │   │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
    ↑           ↑
  tail=0      head=3
  
返回: 3 (写入 3 字节)
```

**场景2：环绕写入**
```
初始状态 (已有部分数据，读取了一些):
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │ C │ D │ E │ F │ G │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
                ↑                   ↑
              tail=2              head=7

调用: ringbuff_write(rb, "HIJK", 4)

计算空闲空间:
  tail(2) > head(7)? 否
  free_space = size - head + tail = 8 - 7 + 2 = 3
  len = min(4, 3) = 3  // 只能写3个字节

写入过程:
  first_part = min(8-7, 3) = 1  // 末尾只能写1个
  memcpy(data+7, "H", 1)        // 写入 H
  memcpy(data+0, "IJ", 2)       // 环绕，写入 IJ

更新指针:
  head = (7 + 3) % 8 = 2

结果:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ I │ J │ C │ D │ E │ F │ G │ H │
   └───┴───┴───┴───┴───┴───┴───┴───┘
        ↑   ↑
      head=2 tail=2  (注意: full = 1)
  
返回: 3 (只写入 3 字节，因为空间不足)
```

**使用示例**：
```c
ringbuff_t rb;
ringbuff_init(&rb, 1024);

char data[] = "Hello, World!";
int written = ringbuff_write(&rb, data, strlen(data));
printf("写入 %d 字节\n", written);  // 输出: 写入 13 字节
```

---

### 3. ringbuff_read_ptr - 零拷贝读取（获取指针）

**函数签名**：
```c
int ringbuff_read_ptr(ringbuff_t *rb, const void **ptr, size_t *len);
```

**功能**：获取可读数据的指针和长度，**不移动读指针**（零拷贝）

**代码详解**：
```c
int ringbuff_read_ptr(ringbuff_t *rb, const void **ptr, size_t *len)
{
    // 步骤1: 参数校验
    if (!rb || !ptr || !len)
        return -1;

    // 步骤2: 读取当前指针位置
    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);  // 对方的写指针
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);  // 我的读指针

    // 步骤3: 计算可读数据长度
    size_t data_len;
    if (rb->full)
    {
        data_len = rb->size;  // 缓冲区满
    }
    else if (head >= tail)
    {
        data_len = head - tail;  // 连续数据
    }
    else
    {
        data_len = rb->size - tail + head;  // 环绕数据
    }

    if (data_len == 0)
        return -2;  // 无数据可读

    // 步骤4: 计算连续可读长度
    // 注意: 只返回 tail 到缓冲区末尾的连续部分！
    size_t max_read = rb->size - tail;  // tail 到末尾的长度
    if (data_len < max_read)
        max_read = data_len;  // 如果数据不足，返回实际长度

    // 步骤5: 返回指针和长度
    *ptr = rb->data + tail;  // 指向 tail 位置
    *len = max_read;         // 连续可读长度

    return 0;
}
```

**关键点：为什么只返回连续部分？**

这是零拷贝的核心设计：
- 只返回从 `tail` 到缓冲区末尾的连续内存
- 如果数据跨越了缓冲区末尾（环绕），需要调用两次才能读完
- 避免了内存拷贝，直接操作原始缓冲区

**可视化执行流程**：

**场景1：连续数据（未环绕）**
```
缓冲区状态:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │ C │ D │ E │ F │   │   │  size=8
   └───┴───┴───┴───┴───┴───┴───┴───┘
                ↑           ↑
              tail=2      head=6

调用: ringbuff_read_ptr(&rb, &ptr, &len)

计算可读长度:
  head(6) >= tail(2)? 是
  data_len = head - tail = 6 - 2 = 4

计算连续可读:
  max_read = min(size - tail, data_len)
           = min(8 - 2, 4)
           = min(6, 4) = 4

结果:
  ptr  指向 → data[2] ('C')
  len = 4
  
  应用可直接访问:
  ptr[0] = 'C'
  ptr[1] = 'D'
  ptr[2] = 'E'
  ptr[3] = 'F'

返回: 0 (成功)
```

**场景2：环绕数据（第一次调用）**
```
缓冲区状态:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ I │ J │ K │   │   │ E │ F │ G │  size=8
   └───┴───┴───┴───┴───┴───┴───┴───┘
            ↑           ↑
          head=3      tail=5

调用: ringbuff_read_ptr(&rb, &ptr, &len)

计算可读长度:
  head(3) >= tail(5)? 否
  data_len = size - tail + head = 8 - 5 + 3 = 6
  (实际数据: E F G I J K)

计算连续可读:
  max_read = min(size - tail, data_len)
           = min(8 - 5, 6)
           = min(3, 6) = 3  // 只返回 tail 到末尾的 3 字节

结果:
  ptr  指向 → data[5] ('E')
  len = 3  // 注意: 只有 EFG，未包含 IJK
  
  应用可访问:
  ptr[0] = 'E'
  ptr[1] = 'F'
  ptr[2] = 'G'

  要读取 IJK，需要:
  1. 调用 ringbuff_consume(&rb, 3)  // 消费 EFG
  2. 再次调用 ringbuff_read_ptr()   // 获取 IJK

返回: 0 (成功)
```

**使用示例**：
```c
ringbuff_t rb;
ringbuff_init(&rb, 1024);

// ... 写入一些数据 ...

// 零拷贝读取
const void *data_ptr;
size_t data_len;

if (ringbuff_read_ptr(&rb, &data_ptr, &data_len) == 0) {
    // 直接使用指针，无需拷贝
    printf("读取到 %zu 字节: %.*s\n", data_len, (int)data_len, (char*)data_ptr);
    
    // 处理完后，标记为已消费
    ringbuff_consume(&rb, data_len);
}
```

---

### 4. ringbuff_consume - 消费数据（移动读指针）

**函数签名**：
```c
void ringbuff_consume(ringbuff_t *rb, size_t len);
```

**功能**：标记数据已被处理，移动读指针（释放空间）

**代码详解**：
```c
void ringbuff_consume(ringbuff_t *rb, size_t len)
{
    // 步骤1: 参数校验
    if (!rb || len == 0)
        return;

    // 步骤2: 读取当前指针位置
    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);

    // 步骤3: 计算当前可用数据量
    size_t data_len;
    if (rb->full)
    {
        data_len = rb->size;
    }
    else if (head >= tail)
    {
        data_len = head - tail;
    }
    else
    {
        data_len = rb->size - tail + head;
    }

    // 步骤4: 处理消费长度超出可用数据的情况
    if (len >= data_len)
    {
        // 消费全部数据，重置缓冲区
        __atomic_store_n(&rb->tail, 0, __ATOMIC_RELEASE);
        __atomic_store_n(&rb->head, 0, __ATOMIC_RELAXED);
        rb->full = 0;
        return;
    }

    // 步骤5: 移动 tail 指针
    tail = (tail + len) % rb->size;  // 环绕
    __atomic_store_n(&rb->tail, tail, __ATOMIC_RELEASE);
    
    // 步骤6: 清除满标志
    rb->full = 0;  // 消费后必然不满
}
```

**可视化执行流程**：

**场景1：正常消费**
```
初始状态:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │ C │ D │ E │ F │   │   │  size=8
   └───┴───┴───┴───┴───┴───┴───┴───┘
                ↑           ↑
              tail=2      head=6

调用: ringbuff_consume(&rb, 2)

计算可用数据:
  data_len = head - tail = 6 - 2 = 4

移动 tail:
  tail = (2 + 2) % 8 = 4

结果:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │ C │ D │ E │ F │   │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
                        ↑   ↑
                      tail=4 head=6
  
  C 和 D 已被"消费"，空间释放
  现在可用数据: E F (2字节)
```

**场景2：环绕消费**
```
初始状态:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ I │ J │   │   │   │ E │ F │ G │  size=8
   └───┴───┴───┴───┴───┴───┴───┴───┘
        ↑                       ↑
      head=2                  tail=5

调用: ringbuff_consume(&rb, 5)

计算可用数据:
  head(2) >= tail(5)? 否
  data_len = size - tail + head = 8 - 5 + 2 = 5
  (数据: E F G I J)

移动 tail:
  tail = (5 + 5) % 8 = 10 % 8 = 2  // 环绕到位置 2

结果:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ I │ J │   │   │   │ E │ F │ G │
   └───┴───┴───┴───┴───┴───┴───┴───┘
        ↑
    tail=2, head=2, full=0
  
  所有数据已消费，缓冲区为空
```

**场景3：消费全部数据（重置）**
```
初始状态:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │ C │ D │ E │ F │   │   │  size=8
   └───┴───┴───┴───┴───┴───┴───┴───┘
                ↑           ↑
              tail=2      head=6

调用: ringbuff_consume(&rb, 100)  // 超过实际数据量

检测到 len(100) >= data_len(4):
  直接重置:
    tail = 0
    head = 0
    full = 0

结果:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │   │   │ C │ D │ E │ F │   │   │
   └───┴───┴───┴───┴───┴───┴───┴───┘
    ↑
  tail=0, head=0
  
  缓冲区重置为空
```

**使用示例**：
```c
const void *data;
size_t len;

// 1. 获取数据指针
if (ringbuff_read_ptr(&rb, &data, &len) == 0) {
    // 2. 处理数据
    process_data(data, len);
    
    // 3. 标记为已消费
    ringbuff_consume(&rb, len);
}
```

---

### 5. ringbuff_read - 拷贝读取数据

**函数签名**：
```c
int ringbuff_read(ringbuff_t *rb, void *dst, size_t len);
```

**功能**：从缓冲区读取数据并拷贝到目标缓冲区，**同时移动读指针**

**代码详解**：
```c
int ringbuff_read(ringbuff_t *rb, void *dst, size_t len)
{
    // 步骤1: 参数校验
    if (!rb || !dst || len == 0)
        return 0;

    // 步骤2: 读取当前指针位置
    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);

    // 步骤3: 计算可读数据量
    size_t data_len;
    if (rb->full)
    {
        data_len = rb->size;
    }
    else if (head >= tail)
    {
        data_len = head - tail;
    }
    else
    {
        data_len = rb->size - tail + head;
    }

    // 步骤4: 调整读取长度
    if (len > data_len)
        len = data_len;  // 最多读取可用数据量

    if (len == 0)
        return 0;  // 无数据可读

    // 步骤5: 处理环绕读取
    size_t first_part = rb->size - tail;  // tail 到末尾的长度
    if (first_part > len)
        first_part = len;  // 如果不需要环绕

    // 拷贝第一部分 (从 tail 开始)
    memcpy(dst, rb->data + tail, first_part);
    
    // 拷贝第二部分 (从缓冲区开头，如果需要环绕)
    memcpy((uint8_t *)dst + first_part, rb->data, len - first_part);

    // 步骤6: 移动 tail 指针
    tail = (tail + len) % rb->size;
    __atomic_store_n(&rb->tail, tail, __ATOMIC_RELEASE);
    
    // 步骤7: 清除满标志
    rb->full = 0;

    return (int)len;  // 返回实际读取的字节数
}
```

**与 ringbuff_read_ptr 的区别**：

| 特性 | ringbuff_read_ptr | ringbuff_read |
|------|-------------------|---------------|
| 内存拷贝 | ❌ 无拷贝（零拷贝） | ✅ 拷贝到目标缓冲区 |
| 移动读指针 | ❌ 不移动 | ✅ 自动移动 |
| 环绕处理 | 只返回连续部分 | 自动处理环绕 |
| 性能 | ⚡ 更快 | 🐢 较慢（有拷贝开销） |
| 使用场景 | 高性能路径 | 简单易用场景 |

**可视化执行流程**：

**场景：环绕读取**
```
初始状态:
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ I │ J │ K │   │   │ E │ F │ G │  size=8
   └───┴───┴───┴───┴───┴───┴───┴───┘
            ↑           ↑
          head=3      tail=5

调用: 
  char buffer[10];
  ringbuff_read(&rb, buffer, 6);

计算可读数据:
  data_len = size - tail + head = 8 - 5 + 3 = 6

读取过程:
  first_part = min(8-5, 6) = 3
  
  // 第一部分: 拷贝 E F G
  memcpy(buffer, data+5, 3)
    buffer[0] = 'E'
    buffer[1] = 'F'
    buffer[2] = 'G'
  
  // 第二部分: 拷贝 I J K
  memcpy(buffer+3, data+0, 3)
    buffer[3] = 'I'
    buffer[4] = 'J'
    buffer[5] = 'K'

更新 tail:
  tail = (5 + 6) % 8 = 3

结果:
  buffer 内容: "EFGIJK"
  
   ┌───┬───┬───┬───┬───┬───┬───┬───┐
   │ I │ J │ K │   │   │ E │ F │ G │
   └───┴───┴───┴───┴───┴───┴───┴───┘
            ↑
        tail=3, head=3  (缓冲区为空)

返回: 6 (读取 6 字节)
```

**使用示例**：
```c
ringbuff_t rb;
ringbuff_init(&rb, 1024);

// ... 写入数据 ...

// 读取数据到本地缓冲区
char buffer[256];
int bytes_read = ringbuff_read(&rb, buffer, sizeof(buffer));
if (bytes_read > 0) {
    printf("读取 %d 字节: %.*s\n", bytes_read, bytes_read, buffer);
}
```

---

### 6. ringbuff_peek - 查看数据（不消费）

**函数签名**：
```c
int ringbuff_peek(ringbuff_t *rb, void *dst, size_t len);
```

**功能**：拷贝数据到目标缓冲区，但**不移动读指针**（适用于预读场景）

**代码详解**：
```c
int ringbuff_peek(ringbuff_t *rb, void *dst, size_t len)
{
    // 与 ringbuff_read 几乎相同，但不更新 tail 指针

    if (!rb || !dst || len == 0)
        return 0;

    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_RELAXED);

    // ... 计算可读数据量（同 ringbuff_read）...

    size_t first_part = rb->size - tail;
    if (first_part > len)
        first_part = len;

    // 拷贝数据
    memcpy(dst, rb->data + tail, first_part);
    memcpy((uint8_t *)dst + first_part, rb->data, len - first_part);

    // 注意: 没有更新 tail 指针！
    // tail 保持不变，数据仍然在缓冲区中

    return (int)len;
}
```

**使用场景**：

```c
// 场景1: 协议解析（需要先查看头部）
char header[4];
if (ringbuff_peek(&rb, header, 4) == 4) {
    uint32_t msg_len = *(uint32_t*)header;
    
    // 检查是否收到完整消息
    if (ringbuff_data_len(&rb) >= msg_len) {
        // 完整消息，正式读取
        char *msg = malloc(msg_len);
        ringbuff_read(&rb, msg, msg_len);
        process_message(msg, msg_len);
        free(msg);
    } else {
        // 消息不完整，等待更多数据
        printf("等待更多数据...\n");
    }
}
```

---

### 7. 辅助函数

#### ringbuff_data_len - 获取可读数据量

```c
size_t ringbuff_data_len(const ringbuff_t *rb)
{
    size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);
    size_t tail = __atomic_load_n(&rb->tail, __ATOMIC_ACQUIRE);

    if (rb->full)
        return rb->size;  // 缓冲区满

    if (head >= tail)
        return head - tail;  // 未环绕
    else
        return rb->size - tail + head;  // 已环绕
}
```

#### ringbuff_free_space - 获取可写空间

```c
size_t ringbuff_free_space(const ringbuff_t *rb)
{
    return rb->size - ringbuff_data_len(rb);
}
```

#### ringbuff_reset - 重置缓冲区

```c
void ringbuff_reset(ringbuff_t *rb)
{
    if (!rb)
        return;

    __atomic_store_n(&rb->head, 0, __ATOMIC_RELAXED);
    __atomic_store_n(&rb->tail, 0, __ATOMIC_RELAXED);
    rb->full = 0;
    // 注意: 不清除 data 内容，只是重置指针
}
```

---

## 实际应用场景

### 1. RX Buffer（接收缓冲区）

在 TCP 连接库中，RX Buffer 用于存储从网络接收的数据：

```c
typedef struct rx_buffer_s
{
    int             pipe_fd[2];    // 用于事件通知
    ringbuff_t      buffer;        // 环形缓冲区
    pthread_mutex_t lock;          // 线程锁
    bool            ready;         // 数据就绪标志
} rx_buffer_t;

// RX 线程写入数据
ssize_t rx_buffer_write(rx_buffer_t *rb, const void *data, size_t len)
{
    pthread_mutex_lock(&rb->lock);
    
    // 检查空间
    if (ringbuff_free_space(&rb->buffer) < len) {
        pthread_mutex_unlock(&rb->lock);
        return 0;  // 缓冲区满
    }
    
    // 写入数据
    ringbuff_write(&rb->buffer, data, len);
    rb->ready = true;
    
    pthread_mutex_unlock(&rb->lock);
    return len;
}

// 应用线程零拷贝读取
int rx_buffer_peek(rx_buffer_t *rb, const void **ptr, size_t *len)
{
    pthread_mutex_lock(&rb->lock);
    int r = ringbuff_read_ptr(&rb->buffer, ptr, len);
    pthread_mutex_unlock(&rb->lock);
    return r;
}

// 应用线程消费数据
void rx_buffer_consume(rx_buffer_t *rb, size_t len)
{
    pthread_mutex_lock(&rb->lock);
    ringbuff_consume(&rb->buffer, len);
    rb->ready = (ringbuff_data_len(&rb->buffer) > 0);
    pthread_mutex_unlock(&rb->lock);
}
```

**工作流程**：

```
[ RX 线程 ]                      [ 应用线程 ]
     │                                 │
     │ 1. 从 socket 接收数据           │
     ├─────────────────────────────────┤
     │ rx_buffer_write(rb, data, len)  │
     │   └─> ringbuff_write()          │
     │                                 │
     │ 2. 通过 pipe 通知应用           │
     │   write(pipe_fd[1], &evt, ...)  │
     │                                 │
     ├─────────────────────────────────┤
     │                                 │ 3. 应用收到通知
     │                                 │ epoll_wait()
     │                                 │
     │                                 │ 4. 零拷贝读取
     │                                 │ rx_buffer_peek(&ptr, &len)
     │                                 │   └─> ringbuff_read_ptr()
     │                                 │
     │                                 │ 5. 处理数据
     │                                 │ process_data(ptr, len)
     │                                 │
     │                                 │ 6. 标记已消费
     │                                 │ rx_buffer_consume(len)
     │                                 │   └─> ringbuff_consume()
```

### 2. TX Buffer（发送缓冲区）

TX Buffer 有特殊设计，每个消息带有长度头：

```c
typedef struct tx_buffer_s
{
    int             pipe_fd[2];
    ringbuff_t      buffer;
    pthread_mutex_t lock;
    bool            ready;
} tx_buffer_t;

// 应用线程写入（带长度头）
ssize_t tx_buffer_write(tx_buffer_t *tb, const void *data, size_t len)
{
    pthread_mutex_lock(&tb->lock);
    
    uint32_t header = (uint32_t)len;
    size_t total = sizeof(header) + len;
    
    // 检查空间
    if (ringbuff_free_space(&tb->buffer) < total) {
        pthread_mutex_unlock(&tb->lock);
        return -1;
    }
    
    // 写入长度头 + 数据
    ringbuff_write(&tb->buffer, &header, sizeof(header));
    ringbuff_write(&tb->buffer, data, len);
    tb->ready = true;
    
    pthread_mutex_unlock(&tb->lock);
    return len;
}

// TX 线程读取下一个消息
int tx_buffer_peek_next(tx_buffer_t *tb, const void **ptr, size_t *len)
{
    pthread_mutex_lock(&tb->lock);
    
    const void *hdr_ptr;
    size_t hdr_len;
    
    // 1. 读取长度头
    if (ringbuff_read_ptr(&tb->buffer, &hdr_ptr, &hdr_len) != 0 
        || hdr_len < sizeof(uint32_t)) {
        pthread_mutex_unlock(&tb->lock);
        return -1;
    }
    
    uint32_t msg_len;
    memcpy(&msg_len, hdr_ptr, sizeof(uint32_t));
    
    // 2. 消费长度头
    ringbuff_consume(&tb->buffer, sizeof(uint32_t));
    
    // 3. 获取消息体指针
    ringbuff_read_ptr(&tb->buffer, ptr, len);
    *len = msg_len;
    
    pthread_mutex_unlock(&tb->lock);
    return 0;
}
```

**TX Buffer 数据布局**：

```
缓冲区内容:
┌────────┬──────────────┬────────┬──────────────┬────────┐
│ len1=5 │  "Hello"     │ len2=5 │  "World"     │ len3=3 │ ...
│ (4字节)│   (5字节)    │ (4字节)│   (5字节)    │ (4字节)│
└────────┴──────────────┴────────┴──────────────┴────────┘

读取流程:
1. 读取 len1 (4字节) → 得知消息长度为 5
2. 消费长度头 (4字节)
3. 读取消息体 (5字节) → "Hello"
4. 消费消息体 (5字节)
5. 重复...
```

---

## 常见问题

### Q1: 为什么需要 full 标志？

**A**: 当 `head == tail` 时，有两种情况：
1. **空**：初始状态或读完了所有数据
2. **满**：写满了整个缓冲区

没有 `full` 标志，无法区分这两种状态。

**示例**：
```c
// 场景1: 空
head=0, tail=0, full=0  → 数据量 = 0

// 场景2: 满
head=0, tail=0, full=1  → 数据量 = size
```

### Q2: 为什么 ringbuff_read_ptr 只返回连续部分？

**A**: 这是零拷贝的核心设计：
- 返回指针而不是拷贝数据，避免内存拷贝开销
- 环形缓冲区中，数据可能跨越末尾（不连续）
- 返回连续部分，保证指针指向的内存是连续的
- 应用可以调用多次来读取完整数据

**处理环绕数据的正确方式**：
```c
// 方式1: 多次调用（零拷贝）
const void *ptr1, *ptr2;
size_t len1, len2;

if (ringbuff_read_ptr(&rb, &ptr1, &len1) == 0) {
    process_data(ptr1, len1);
    ringbuff_consume(&rb, len1);
    
    // 如果还有数据，继续读取
    if (ringbuff_read_ptr(&rb, &ptr2, &len2) == 0) {
        process_data(ptr2, len2);
        ringbuff_consume(&rb, len2);
    }
}

// 方式2: 使用 ringbuff_read（有拷贝）
char buffer[1024];
int len = ringbuff_read(&rb, buffer, sizeof(buffer));
// ringbuff_read 自动处理环绕，一次性读取
```

### Q3: 为什么使用原子操作？

**A**: 支持多线程环境下的安全访问：
- **生产者线程**更新 `head` 指针
- **消费者线程**更新 `tail` 指针
- 原子操作保证指针读写的原子性和可见性
- 内存序保证操作顺序的正确性

**内存序说明**：
```c
// 生产者写入数据
memcpy(rb->data + head, src, len);  // 1. 写入数据
__atomic_store_n(&rb->head, new_head, __ATOMIC_RELEASE);  // 2. 更新 head
// RELEASE 保证步骤1在步骤2之前完成

// 消费者读取数据
size_t head = __atomic_load_n(&rb->head, __ATOMIC_ACQUIRE);  // 1. 读取 head
memcpy(dst, rb->data + tail, len);  // 2. 读取数据
// ACQUIRE 保证步骤2在步骤1之后执行
```

### Q4: 缓存行填充的作用是什么？

**A**: 避免**伪共享（False Sharing）**问题：

```c
// 没有填充的情况
struct ringbuff_bad {
    uint8_t *data;
    size_t head;  // 生产者频繁修改
    size_t tail;  // 消费者频繁修改
};
// 如果 head 和 tail 在同一缓存行（64字节），会导致缓存失效

// 有填充的情况
struct ringbuff_good {
    uint8_t *data;
    size_t head;
    size_t tail;
    char _pad[48];  // 填充到 64 字节
};
// 每个 ringbuff_t 实例独占缓存行，避免缓存冲突
```

### Q5: 如何选择缓冲区大小？

**A**: 考虑以下因素：

1. **应用需求**：
   - 高吞吐场景：1MB - 10MB
   - 低延迟场景：64KB - 256KB
   - 嵌入式系统：4KB - 64KB

2. **内存限制**：
   - 每个连接都有 RX 和 TX 缓冲区
   - 总内存 = 连接数 × (RX_SIZE + TX_SIZE)

3. **性能权衡**：
   - 过小：频繁满/空，降低吞吐
   - 过大：浪费内存，增加延迟

**示例配置**：
```json
{
    "global_settings": {
        "ring_buffer_size": 8192  // 8KB，适合一般场景
    }
}
```

### Q6: ringbuff_write 返回值小于请求长度怎么办？

**A**: 说明缓冲区空间不足，只写入了部分数据：

```c
char data[1000];
int written = ringbuff_write(&rb, data, 1000);

if (written < 1000) {
    // 方案1: 重试写入剩余部分
    int remaining = 1000 - written;
    int retry_written = ringbuff_write(&rb, data + written, remaining);
    
    // 方案2: 记录错误并丢弃
    LOG_ERROR("缓冲区满，丢弃 %d 字节", 1000 - written);
    
    // 方案3: 等待空间释放
    while (written < 1000) {
        usleep(1000);  // 等待 1ms
        int retry = ringbuff_write(&rb, data + written, 1000 - written);
        written += retry;
    }
}
```

---

## 性能优化总结

### 1. 零拷贝设计
- 使用 `ringbuff_read_ptr` 直接返回指针
- 避免 `memcpy` 开销

### 2. 原子操作
- 无锁读写（单生产者单消费者）
- 降低锁竞争

### 3. 缓存行对齐
- 避免伪共享
- 提升多核性能

### 4. 内存连续性
- 固定大小的连续内存
- 减少 page fault

### 5. 批量操作
- 一次性写入/读取大块数据
- 减少函数调用开销

---

## 完整使用示例

### 示例1：生产者-消费者模型

```c
#include "ringbuff.h"
#include <pthread.h>
#include <stdio.h>

ringbuff_t g_buffer;

// 生产者线程
void *producer_thread(void *arg)
{
    char message[100];
    for (int i = 0; i < 100; i++) {
        snprintf(message, sizeof(message), "Message %d", i);
        
        // 写入数据
        while (ringbuff_write(&g_buffer, message, strlen(message)) == 0) {
            usleep(1000);  // 等待空间释放
        }
        
        printf("[生产者] 写入: %s\n", message);
        usleep(10000);  // 模拟生产延迟
    }
    return NULL;
}

// 消费者线程
void *consumer_thread(void *arg)
{
    const void *data;
    size_t len;
    
    while (1) {
        // 零拷贝读取
        if (ringbuff_read_ptr(&g_buffer, &data, &len) == 0) {
            printf("[消费者] 读取 %zu 字节: %.*s\n", len, (int)len, (char*)data);
            
            // 标记已消费
            ringbuff_consume(&g_buffer, len);
        } else {
            usleep(1000);  // 无数据，等待
        }
    }
    return NULL;
}

int main()
{
    // 初始化缓冲区
    ringbuff_init(&g_buffer, 4096);
    
    // 启动线程
    pthread_t producer, consumer;
    pthread_create(&producer, NULL, producer_thread, NULL);
    pthread_create(&consumer, NULL, consumer_thread, NULL);
    
    // 等待生产者完成
    pthread_join(producer, NULL);
    
    // 清理
    ringbuff_free(&g_buffer);
    return 0;
}
```

### 示例2：网络数据接收

```c
// 网络接收线程
void network_receive_loop(int sockfd, ringbuff_t *rx_buffer)
{
    char temp_buf[4096];
    
    while (1) {
        // 从 socket 接收数据
        ssize_t received = recv(sockfd, temp_buf, sizeof(temp_buf), 0);
        if (received <= 0) {
            break;  // 连接关闭或错误
        }
        
        // 写入环形缓冲区
        while (ringbuff_write(rx_buffer, temp_buf, received) < received) {
            usleep(100);  // 等待应用消费数据
        }
    }
}

// 应用处理线程
void application_process_loop(ringbuff_t *rx_buffer)
{
    const void *data;
    size_t len;
    
    while (1) {
        // 零拷贝读取
        if (ringbuff_read_ptr(rx_buffer, &data, &len) == 0) {
            // 直接处理数据，无需拷贝
            process_network_data(data, len);
            
            // 标记已处理
            ringbuff_consume(rx_buffer, len);
        }
    }
}
```

---

## 总结

环形缓冲区是高性能系统中的核心数据结构，本项目的实现具有以下特点：

✅ **高效**：零拷贝、原子操作、缓存行对齐  
✅ **安全**：线程安全、内存序保证  
✅ **灵活**：支持零拷贝和拷贝两种模式  
✅ **实用**：已在 TCP 连接库中验证

通过本教学文档，你应该能够：
1. 理解环形缓冲区的基本原理
2. 掌握每个函数的工作流程
3. 在实际项目中应用 ringbuff

**关键要点回顾**：
- `head` 是写指针，`tail` 是读指针
- `full` 标志区分满和空状态
- `ringbuff_read_ptr` 零拷贝，`ringbuff_read` 有拷贝
- `ringbuff_consume` 必须在处理完数据后调用
- 原子操作保证多线程安全
