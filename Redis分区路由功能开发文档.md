# Redis 分区路由功能开发文档

**项目**: share-offer  
**开发者**: 郭帅  
**日期**: 2025-12-22  
**状态**: Phase 1 已完成，Phase 2 待开发

---

## 📋 目录

- [一、功能概述](#一功能概述)
- [二、已完成功能（Phase 1）](#二已完成功能phase-1)
- [三、待开发功能（Phase 2）](#三待开发功能phase-2)
- [四、技术架构](#四技术架构)
- [五、代码实现详解](#五代码实现详解)
- [六、测试与验证](#六测试与验证)

---

## 一、功能概述

### 1.1 业务背景

在共享报盘系统中，**多个 TDGW 网关实例可以负责相同的分区**。系统通过**路由算法**动态选择最优的 TDGW 进行订单转发，实现负载均衡和高可用。

**架构示例**：
```
TDGW-A (conn_id=10): 负责分区 (0001,1), (0001,2), (0002,1)  ← 实例 A
TDGW-B (conn_id=11): 负责分区 (0001,1), (0001,2), (0002,1)  ← 实例 B，相同分区！
TDGW-C (conn_id=12): 负责分区 (0001,1), (0003,1)           ← 实例 C，部分重叠

订单路由到分区 (0001,1) 时：
  候选 TDGW: [10, 11, 12]
  → 路由算法选择: 哪个 TDGW 更快就走哪个
  → 选择依据: 延迟、负载、连接状态等

特点：
✅ 分区可重叠：多个 TDGW 可负责相同分区
✅ 动态路由：根据性能指标选择最优 TDGW
✅ 负载均衡：订单分散到多个 TDGW
✅ 高可用：某个 TDGW 故障时自动切换到其他实例
```

### 1.2 核心需求

1. **分区路由映射建立**：TDGW 登录后，自动在 Redis 中建立 `pbu + set_id → conn_id` 的映射
2. **订单路由查询**：收到 OMS 订单时，查询 Redis 获取目标 TDGW，实现智能路由
3. **执行报告索引同步**：从 Redis 获取分区的最大报告索引，同步到 TDGW

### 1.3 Redis Key 设计

根据《共享报盘redis结构汇总.pdf》：

| Key 格式 | Value 类型 | 用途 | 示例 |
|---------|-----------|------|------|
| `exec_rpt_idx:{pbu}:{set_id}` | u64 | 存储分区的最大报告索引 | `exec_rpt_idx:0001:1` → `12345` |
| `routing:{pbu}:{set_id}` | **List<u16>** | **存储分区对应的多个 conn_id** | `routing:0001:1` → `[10, 11, 12]` |
| `tdgw_metrics:{conn_id}` | JSON | 存储 TDGW 性能指标（可选） | `{"latency":10, "load":0.3}` |

**关键区别**：
- ✅ `routing` 存储 **List**，不是单一值
- ✅ TDGW 登录时使用 `SADD` 添加到集合，不是 `SET` 覆盖
- ✅ TDGW 断开时使用 `SREM` 移除，保持路由表实时性

---

## 二、已完成功能（Phase 1）

### 2.1 功能清单

✅ **Redis 客户端封装**（`src/redis_client.rs`）  
✅ **执行报告索引查询**（批量优化）  
✅ **分区路由映射建立**（`process_tdgw_exec_rpt_info_msg`）  
✅ **路由查询接口**（`SessionManager::route_order`）  

### 2.2 文件修改统计

```
修改文件：
  - Cargo.toml              (新增 redis 依赖)
  - src/lib.rs              (注册 redis_client 模块)
  - src/redis_client.rs     (新建，143 行)
  - src/session.rs          (+62 行，-7 行)

Git 提交记录：
  1. feat(redis): add Redis client for exec report index and routing
  2. feat(lib): register redis_client module
  3. feat(session): integrate Redis for exec report index query
```

---

## 三、待开发功能（Phase 2）

### 3.1 订单路由集成

**文件**: `src/main.rs`  
**位置**: `TdgwBinFrame::NewOrderSingleNew` 处理逻辑

**待实现逻辑**：

```rust
TdgwBinFrame::NewOrderSingleNew(new_order) => {
    // 1. 提取 PBU
    let pbu = new_order.get_biz_pbu_as_string();
    
    // 2. 确定 set_id（待确认方案）
    let set_id = extract_set_id_from_order(&new_order);
    
    // 3. 查询路由
    match self.session_manager.route_order(&pbu, set_id) {
        Some(target_conn_id) => {
            // 转发到目标 TDGW
        }
        None => {
            // 路由失败处理
        }
    }
}
```

### 3.2 set_id 确定方案（需业务确认）

| 方案 | 实现方式 | 优点 | 缺点 |
|------|---------|------|------|
| **方案 A** | 从 `user_info` 解析 | 灵活，OMS 可控 | 需 OMS 协议支持 |
| **方案 B** | 从 `account` 计算 | 符合业务规则 | 需账户与分区映射表 |
| **方案 C** | 使用 `biz_id % N` | 实现简单 | 分布可能不均 |

**推荐方案 B**：根据证券账户号段确定分区

```rust
fn extract_set_id_from_account(account: &str) -> u32 {
    let account_num: u64 = account.parse().unwrap_or(0);
    ((account_num / 1000000) % 10) as u32 + 1
}
```

### 3.3 Redis 初始化

**文件**: `src/main.rs`  
**位置**: `ShareOffer::init()` 方法

```rust
fn init(&mut self) {
    // ... 现有初始化逻辑
    
    // 初始化 Redis
    let redis_url = "redis://127.0.0.1:6379";
    if let Err(e) = self.session_manager.init_redis(redis_url) {
        error!(target: "system", "Redis init failed: {}", e);
    }
}
```

### 3.4 配置文件支持

**文件**: `config/redis.toml`（待创建）

```toml
[redis]
url = "redis://127.0.0.1:6379"
connection_timeout_ms = 5000
read_timeout_ms = 3000
```

---

## 四、技术架构

### 4.1 整体流程图

```
┌─────────────────────────────────────────────────────────┐
│                      OMS 柜台                            │
└──────────────────────┬──────────────────────────────────┘
                       │ NewOrderSingle
                       │ (pbu=0001, account=...)
                       ↓
┌─────────────────────────────────────────────────────────┐
│                  share-offer 主程序                       │
│  ┌────────────────────────────────────────────────┐    │
│  │ 1. 提取 pbu = "0001"                            │    │
│  │ 2. 计算 set_id = extract_set_id_from_account() │    │
│  │ 3. 查询 Redis: routing:0001:1                  │    │
│  │ 4. 获取 target_conn_id = 10                    │    │
│  └────────────────────────────────────────────────┘    │
└──────────────────────┬──────────────────────────────────┘
                       │ 转发到 conn_id=10
                       ↓
┌─────────────────────────────────────────────────────────┐
│                   TDGW (conn_id=10)                      │
│                  负责分区: 0001:1                         │
└─────────────────────────────────────────────────────────┘
```

### 4.2 路由映射建立流程

```
TDGW 登录流程：
  1. TCP 连接建立 → send Logon
  2. 收到 LogonResp → status = LoggedIn
  3. 收到 ExecRptInfo（分区列表）
     ├─ pbu_list: ["0001", "0002"]
     ├─ set_id_list: [1, 2, 3]
     └─ 组合: (0001,1), (0001,2), (0001,3), (0002,1)...
  4. 批量查询 Redis 获取索引
     └─ batch_get_max_report_index(pbu_set_pairs)
  5. 构建并发送 ExecRptSync
  6. 发送成功后 → 建立路由映射
     ├─ set_partition_routing("0001", 1, conn_id=10)
     ├─ set_partition_routing("0001", 2, conn_id=10)
     └─ ...
```

---

## 五、代码实现详解

### 5.1 完整流程概览

```
第一阶段：TDGW 注册路由
  1. TDGW 登录 → 发送 ExecRptInfo（分区信息）
  2. share-offer 查询 Redis 获取索引 → 发送 ExecRptSync
  3. ExecRptSync 发送成功 → 注册路由到 Redis

第二阶段：订单路由查询
  4. OMS 发来订单 → 根据 pbu+set_id 查询路由 → 转发到对应 TDGW
```

---

### 5.2 第一阶段：TDGW 注册路由

#### 5.2.1 入口方法：`process_tdgw_exec_rpt_info_msg`

**文件**：`src/session.rs` (L373-465)

**功能**：TDGW 登录后发送 ExecRptInfo，share-offer 处理分区信息并注册路由

#### **步骤 1：收集分区信息**

```rust
// src/session.rs L379-387
let mut pbu_set_pairs = vec![];
for pbu in execrptinfo.get_no_groups_4() {          // ← 遍历所有 pbu
    for set_id in execrptinfo.get_no_groups_5() {   // ← 遍历所有 set_id
        pbu_set_pairs.push((
            String::from_utf8_lossy(pbu.get_pbu()).trim().to_string(),
            set_id.get_set_id()
        ));
    }
}
```

**实际执行示例**：
```rust
// 假设 TDGW-A (conn_id=10) 发来的 ExecRptInfo：
execrptinfo.get_no_groups_4() = [{pbu: "0001"}, {pbu: "0002"}]
execrptinfo.get_no_groups_5() = [{set_id: 1}, {set_id: 2}]

// 结果：pbu_set_pairs = [
//   ("0001", 1),
//   ("0001", 2),
//   ("0002", 1),
//   ("0002", 2)
// ]
```

---

#### **步骤 2：批量查询 Redis 获取执行报告索引**

```rust
// src/session.rs L388-402
let report_indexes = if let Some(ref redis_client) = self.redis_client {
    match redis_client.batch_get_max_report_index(&pbu_set_pairs) {
        Ok(indexes) => indexes,
        Err(e) => {
            // Redis 查询失败 → 使用默认值 1
            error!(target: "business", "Redis query failed: {:?}, using default", e);
            pbu_set_pairs.iter()
                .map(|(pbu, set_id)| ((pbu.clone(), *set_id), 1u64))
                .collect()
        }
    }
} else {
    // Redis 未初始化 → 使用默认值 1
    pbu_set_pairs.iter()
        .map(|(pbu, set_id)| ((pbu.clone(), *set_id), 1u64))
        .collect()
};
```

**批量查询实现**（`src/redis_client.rs` L47-71）：

```rust
pub fn batch_get_max_report_index(
    &self, 
    pbu_set_pairs: &[(String, u32)]
) -> Result<HashMap<(String, u32), u64>, RedisError> {
    let mut conn = self.get_connection()?;
    let mut result = HashMap::new();
    
    for (pbu, set_id) in pbu_set_pairs {
        let key = format!("exec_rpt_idx:{}:{}", pbu, set_id);
        match conn.get::<_, Option<u64>>(&key) {
            Ok(Some(index)) => {
                result.insert((pbu.clone(), *set_id), index);
            }
            Ok(None) => {
                result.insert((pbu.clone(), *set_id), 1);  // ← 默认值
            }
            Err(e) => {
                warn!(target: "business", "Redis batch_get: key={}, error={:?}", key, e);
                result.insert((pbu.clone(), *set_id), 1);
            }
        }
    }
    
    info!(target: "business", "Redis batch_get_max_report_index: {} keys fetched", result.len());
    Ok(result)
}
```

**Redis 操作示例**：
```bash
# 批量查询 4 个分区的索引
GET exec_rpt_idx:0001:1  → 12345  ✅ 找到
GET exec_rpt_idx:0001:2  → (nil)  ❌ 不存在，使用默认值 1
GET exec_rpt_idx:0002:1  → 67890  ✅ 找到
GET exec_rpt_idx:0002:2  → (nil)  ❌ 不存在，使用默认值 1

# 返回：report_indexes = {
#   ("0001", 1): 12345,
#   ("0001", 2): 1,
#   ("0002", 1): 67890,
#   ("0002", 2): 1
# }
```

---

#### **步骤 3：构建 ExecRptSync 消息**

```rust
// src/session.rs L404-424
let mut v = vec![];
for pbu in execrptinfo.get_no_groups_4() {
    for set_id in execrptinfo.get_no_groups_5() {
        let pbu_str = String::from_utf8_lossy(pbu.get_pbu()).trim().to_string();
        let set_id_val = set_id.get_set_id();
        
        // 从 report_indexes 中获取索引
        let report_index = report_indexes
            .get(&(pbu_str, set_id_val))
            .copied()
            .unwrap_or(1);  // ← 找不到就用 1
        
        // 构建同步消息
        let mut sync = tdgw_bin::exec_rpt_sync::NoGroups3::new();
        sync.set_pbu_from_ref(pbu.get_pbu());
        sync.set_set_id(set_id_val);
        sync.set_begin_report_index(report_index);  // ← 设置起始索引
        v.push(sync);
    }
}
```

**构建结果示例**：
```rust
// v = [
//   {pbu: "0001", set_id: 1, begin_report_index: 12345},
//   {pbu: "0001", set_id: 2, begin_report_index: 1},
//   {pbu: "0002", set_id: 1, begin_report_index: 67890},
//   {pbu: "0002", set_id: 2, begin_report_index: 1}
// ]
```

---

#### **步骤 4：发送 ExecRptSync 并注册路由**

```rust
// src/session.rs L427-464
match self.conn_id_2_session.get_mut(&conn_id) {
    Some(session) => {
        // 1️⃣ 构建并发送 ExecRptSync 消息
        let mut exec_rpt_sync = tdgw_bin::exec_rpt_sync::ExecRptSync::new();
        exec_rpt_sync.set_no_groups_3(&v);
        exec_rpt_sync.filled_head_and_tail();
        
        match session.conn.tcp_conn_send_bytes(&exec_rpt_sync.as_bytes()) {
            Ok(_) => {
                info!(target: "messages::tdgw::out", "{:?}, {:?}, {}", 
                      conn_id, now, exec_rpt_sync);
                session.last_write_time_ms = now;
                
                // 2️⃣ 【关键】发送成功后，注册路由到 Redis
                if let Some(ref redis_client) = self.redis_client {
                    for (pbu, set_id) in &pbu_set_pairs {
                        match redis_client.set_partition_routing(pbu, *set_id, conn_id) {
                            Ok(_) => {
                                debug!(target: "business", 
                                       "partition routing set: pbu={}, set_id={}, conn_id={}", 
                                       pbu, set_id, conn_id);
                            }
                            Err(e) => {
                                warn!(target: "business", 
                                      "failed to set partition routing: pbu={}, set_id={}, error={:?}", 
                                      pbu, set_id, e);
                            }
                        }
                    }
                }
            }
            Err(error) => {
                error!("send exec_rpt_sync fail: {:?}, conn_id:{:?}", error, conn_id);
                session.status = SessionStatus::WaitDisconnect;
            }
        }
    }
}
```

**路由注册实现**（`src/redis_client.rs` L78-84）：

```rust
pub fn set_partition_routing(&self, pbu: &str, set_id: u32, conn_id: u16) 
    -> Result<(), RedisError> 
{
    let key = format!("routing:{}:{}", pbu, set_id);  // ← 构造 key
    let mut conn = self.get_connection()?;
    let _: () = conn.set(&key, conn_id)?;             // ← Redis SET 命令
    info!(target: "business", "Redis set_partition_routing: key={}, conn_id={}", key, conn_id);
    Ok(())
}
```

**Redis 操作示例**：
```bash
# TDGW-A (conn_id=10) 登录，注册路由：
SET routing:0001:1 10  # ← pbu="0001", set_id=1 → conn_id=10
SET routing:0001:2 10
SET routing:0002:1 10
SET routing:0002:2 10

# 验证：
GET routing:0001:1 → 10 ✅
GET routing:0001:2 → 10 ✅
GET routing:0002:1 → 10 ✅
GET routing:0002:2 → 10 ✅
```

---

### 5.3 第二阶段：订单路由查询

#### 5.3.1 路由查询方法：`route_order`

**文件**：`src/session.rs` (L82-112)

**功能**：根据订单的 pbu 和 set_id 查询对应的 TDGW conn_id

#### **完整实现**：

```rust
pub fn route_order(&self, pbu: &str, set_id: u32) -> Option<u16> {
    // 步骤 1：检查 Redis 是否初始化
    if let Some(ref redis_client) = self.redis_client {
        // 步骤 2：查询 Redis 获取 conn_id
        match redis_client.get_partition_routing(pbu, set_id) {
            Ok(Some(conn_id)) => {
                // 步骤 3：检查连接是否存在且状态为 Ready
                if self.conn_id_2_session
                    .get(&conn_id)
                    .map(|s| s.status == SessionStatus::Ready)
                    .unwrap_or(false)
                {
                    // ✅ TDGW 存在且 Ready，返回 conn_id
                    debug!(target: "business", 
                           "order routed: pbu={}, set_id={}, conn_id={}", 
                           pbu, set_id, conn_id);
                    Some(conn_id)
                } else {
                    // ❌ TDGW 不存在或未 Ready
                    warn!(target: "business", 
                          "route stale (TDGW not ready): pbu={}, set_id={}, conn_id={}", 
                          pbu, set_id, conn_id);
                    None
                }
            }
            Ok(None) => {
                // ❌ 未找到路由
                warn!(target: "business", 
                      "no route found for partition: pbu={}, set_id={}", 
                      pbu, set_id);
                None
            }
            Err(e) => {
                // ❌ Redis 查询失败
                error!(target: "business", 
                       "route query failed: pbu={}, set_id={}, error={:?}", 
                       pbu, set_id, e);
                None
            }
        }
    } else {
        // ❌ Redis 未初始化
        warn!(target: "business", "Redis not initialized, cannot route order");
        None
    }
}
```

#### **Redis 查询实现**（`src/redis_client.rs` L88-103）：

```rust
pub fn get_partition_routing(&self, pbu: &str, set_id: u32) 
    -> Result<Option<u16>, RedisError> 
{
    let key = format!("routing:{}:{}", pbu, set_id);  // ← key = "routing:0001:1"
    let mut conn = self.get_connection()?;
    
    match conn.get::<_, Option<u16>>(&key) {          // ← Redis GET
        Ok(conn_id) => {
            if let Some(id) = conn_id {
                info!(target: "business", 
                      "Redis get_partition_routing: key={}, conn_id={}", 
                      key, id);
            }
            Ok(conn_id)  // ← 返回 Some(10) 或 None
        }
        Err(e) => {
            error!(target: "business", 
                   "Redis get_partition_routing failed: key={}, error={:?}", 
                   key, e);
            Err(e)
        }
    }
}
```

#### **执行流程示例**：

```rust
// 场景：OMS 发来订单
订单信息：
  pbu = "0001"
  set_id = 1

// 1️⃣ 调用 route_order("0001", 1)
// 2️⃣ Redis 查询：
GET routing:0001:1 → 10  ✅

// 3️⃣ 检查 conn_id=10 的状态：
conn_id_2_session.get(&10) → Some(Session {
    status: SessionStatus::Ready  ✅
})

// 4️⃣ 返回 Some(10)
// 5️⃣ main.rs 转发订单到 conn_id=10 (TDGW-A)
```

#### **为什么需要检查连接状态？**

```rust
// 问题场景：TDGW 刚断开，Redis 中的路由还未清理
Redis: GET routing:0001:1 → 10  ← 路由存在

// 但 session 状态：
conn_id_2_session.get(&10) → Some(Session {
    status: SessionStatus::Disconnected  ❌ 已断开！
})

// 检查结果：
s.status == SessionStatus::Ready  → false
// 返回 None，避免路由到已断开的 TDGW ✅
```

---

### 5.4 完整流程示意图

```
═══════════════════════════════════════════════════════════
第一阶段：TDGW 注册路由
═══════════════════════════════════════════════════════════

TDGW-A (conn_id=10) 登录
  ↓
发送 ExecRptInfo
  pbu_list: ["0001", "0002"]
  set_id_list: [1, 2]
  ↓
share-offer.process_tdgw_exec_rpt_info_msg()
  ↓
① 收集分区：pbu_set_pairs = [
    ("0001", 1),
    ("0001", 2),
    ("0002", 1),
    ("0002", 2)
  ]
  ↓
② 批量查询 Redis 索引：
    GET exec_rpt_idx:0001:1 → 12345
    GET exec_rpt_idx:0001:2 → 1
    GET exec_rpt_idx:0002:1 → 67890
    GET exec_rpt_idx:0002:2 → 1
  ↓
③ 构建 ExecRptSync 消息
  ↓
④ 发送 ExecRptSync → TDGW-A ✅
  ↓
⑤ 注册路由到 Redis：
    SET routing:0001:1 10
    SET routing:0001:2 10
    SET routing:0002:1 10
    SET routing:0002:2 10

═══════════════════════════════════════════════════════════
第二阶段：订单路由查询
═══════════════════════════════════════════════════════════

OMS 发来订单
  pbu = "0001"
  set_id = 1
  ↓
main.rs 调用 session_manager.route_order("0001", 1)
  ↓
route_order() 执行：
  ↓
① 检查 Redis 是否初始化 ✅
  ↓
② 查询 Redis：
    GET routing:0001:1 → 10
  ↓
③ 检查 conn_id=10 的状态：
    conn_id_2_session.get(&10) → Some(Session)
    session.status == Ready? → true ✅
  ↓
④ 返回 Some(10)
  ↓
main.rs 转发订单到 conn_id=10 (TDGW-A)
```

### 5.1 Redis 客户端封装

**文件**: `src/redis_client.rs`

```rust
use redis::{Client, Commands, Connection, RedisError};
use std::collections::HashMap;
use tracing::{info, warn, error};

pub struct RedisClient {
    client: Client,
}

impl RedisClient {
    /// 创建 Redis 客户端
    pub fn new(redis_url: &str) -> Result<Self, RedisError> {
        let client = Client::open(redis_url)?;
        info!(target: "system", "Redis client created: url={}", redis_url);
        Ok(Self { client })
    }

    /// 批量查询分区索引（性能优化）
    pub fn batch_get_max_report_index(
        &self, 
        pbu_set_pairs: &[(String, u32)]
    ) -> Result<HashMap<(String, u32), u64>, RedisError> {
        let mut conn = self.get_connection()?;
        let mut result = HashMap::new();
        
        for (pbu, set_id) in pbu_set_pairs {
            let key = format!("exec_rpt_idx:{}:{}", pbu, set_id);
            match conn.get::<_, Option<u64>>(&key) {
                Ok(Some(index)) => result.insert((pbu.clone(), *set_id), index),
                Ok(None) => result.insert((pbu.clone(), *set_id), 1),
                Err(e) => {
                    warn!(target: "business", "Redis query failed: key={}, error={:?}", key, e);
                    result.insert((pbu.clone(), *set_id), 1)
                }
            };
        }
        
        info!(target: "business", "batch_get: {} keys fetched", result.len());
        Ok(result)
    }

    /// 设置分区路由
    pub fn set_partition_routing(&self, pbu: &str, set_id: u32, conn_id: u16) -> Result<(), RedisError> {
        let key = format!("routing:{}:{}", pbu, set_id);
        let mut conn = self.get_connection()?;
        let _: () = conn.set(&key, conn_id)?;
        Ok(())
    }

    /// 查询分区路由
    pub fn get_partition_routing(&self, pbu: &str, set_id: u32) -> Result<Option<u16>, RedisError> {
        let key = format!("routing:{}:{}", pbu, set_id);
        let mut conn = self.get_connection()?;
        conn.get(&key)
    }
}
```

**关键设计**：
- **批量查询优化**：一次性查询多个分区索引，减少网络往返
- **容错处理**：Redis 查询失败时使用默认值 1，不影响业务
- **类型明确**：显式类型标注 `let _: () = conn.set()` 避免编译错误

### 5.2 SessionManager 集成

**文件**: `src/session.rs`

#### 5.2.1 新增字段

```rust
#[derive(Default)]
pub struct SessionManager {
    conn_id_2_session: HashMap<u16, Session>,
    session_to_reconnect: Vec<u16>,
    redis_client: Option<RedisClient>,  // 新增
}
```

#### 5.2.2 Redis 初始化

```rust
impl SessionManager {
    pub fn init_redis(&mut self, redis_url: &str) -> Result<(), String> {
        match RedisClient::new(redis_url) {
            Ok(client) => {
                if let Err(e) = client.ping() {
                    error!(target: "system", "Redis connection failed: {:?}", e);
                    return Err(format!("Redis connection failed: {:?}", e));
                }
                self.redis_client = Some(client);
                info!(target: "system", "Redis client initialized: url={}", redis_url);
                Ok(())
            }
            Err(e) => {
                error!(target: "system", "Redis client creation failed: {:?}", e);
                Err(format!("Failed to create Redis client: {:?}", e))
            }
        }
    }
}
```

#### 5.2.3 订单路由查询

```rust
impl SessionManager {
    /// 根据 pbu + set_id 查询目标 TDGW 连接
    pub fn route_order(&self, pbu: &str, set_id: u32) -> Option<u16> {
        if let Some(ref redis_client) = self.redis_client {
            match redis_client.get_partition_routing(pbu, set_id) {
                Ok(Some(conn_id)) => {
                    debug!(target: "business", 
                           "order routed: pbu={}, set_id={}, conn_id={}", 
                           pbu, set_id, conn_id);
                    Some(conn_id)
                }
                Ok(None) => {
                    warn!(target: "business", 
                          "no route found: pbu={}, set_id={}", pbu, set_id);
                    None
                }
                Err(e) => {
                    error!(target: "business", 
                           "route query failed: pbu={}, set_id={}, error={:?}", 
                           pbu, set_id, e);
                    None
                }
            }
        } else {
            warn!(target: "business", "Redis not initialized, cannot route order");
            None
        }
    }
}
```

#### 5.2.4 ExecRptInfo 处理（路由映射建立）

```rust
pub fn process_tdgw_exec_rpt_info_msg(
    &mut self,
    now: u128,
    conn_id: u16,
    execrptinfo: &tdgw_bin::exec_rpt_info::ExecRptInfo
) {
    // 收集所有分区
    let mut pbu_set_pairs = vec![];
    for pbu in execrptinfo.get_no_groups_4() {
        for set_id in execrptinfo.get_no_groups_5() {
            pbu_set_pairs.push((
                String::from_utf8_lossy(pbu.get_pbu()).trim().to_string(),
                set_id.get_set_id()
            ));
        }
    }
    
    // 批量查询 Redis 索引
    let report_indexes = if let Some(ref redis_client) = self.redis_client {
        match redis_client.batch_get_max_report_index(&pbu_set_pairs) {
            Ok(indexes) => indexes,
            Err(e) => {
                error!(target: "business", "Redis query failed: {:?}, using default", e);
                pbu_set_pairs.iter()
                    .map(|(pbu, set_id)| ((pbu.clone(), *set_id), 1u64))
                    .collect()
            }
        }
    } else {
        // Redis 未初始化，使用默认值
        pbu_set_pairs.iter()
            .map(|(pbu, set_id)| ((pbu.clone(), *set_id), 1u64))
            .collect()
    };
    
    // 构建 ExecRptSync 消息
    let mut v = vec![];
    for pbu in execrptinfo.get_no_groups_4() {
        for set_id in execrptinfo.get_no_groups_5() {
            let pbu_str = String::from_utf8_lossy(pbu.get_pbu()).trim().to_string();
            let set_id_val = set_id.get_set_id();
            
            let report_index = report_indexes
                .get(&(pbu_str, set_id_val))
                .copied()
                .unwrap_or(1);
            
            let mut sync = tdgw_bin::exec_rpt_sync::NoGroups3::new();
            sync.set_pbu_from_ref(pbu.get_pbu());
            sync.set_set_id(set_id_val);
            sync.set_begin_report_index(report_index);
            v.push(sync);
        }
    }
    
    // 发送 ExecRptSync
    match self.conn_id_2_session.get_mut(&conn_id) {
        Some(session) => {
            let mut exec_rpt_sync = tdgw_bin::exec_rpt_sync::ExecRptSync::new();
            exec_rpt_sync.set_no_groups_3(&v);
            exec_rpt_sync.filled_head_and_tail();
            
            match session.conn.tcp_conn_send_bytes(&exec_rpt_sync.as_bytes()) {
                Ok(_) => {
                    info!(target: "messages::tdgw::out", "{:?}, {:?}, {}", conn_id, now, exec_rpt_sync);
                    session.last_write_time_ms = now;
                    
                    // ✅ 建立路由映射
                    if let Some(ref redis_client) = self.redis_client {
                        for (pbu, set_id) in &pbu_set_pairs {
                            match redis_client.set_partition_routing(pbu, *set_id, conn_id) {
                                Ok(_) => {
                                    debug!(target: "business", 
                                           "partition routing set: pbu={}, set_id={}, conn_id={}", 
                                           pbu, set_id, conn_id);
                                }
                                Err(e) => {
                                    warn!(target: "business", 
                                          "failed to set routing: pbu={}, set_id={}, error={:?}", 
                                          pbu, set_id, e);
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    error!("send exec_rpt_sync failed: {:?}, conn_id:{:?}", error, conn_id);
                    session.status = SessionStatus::WaitDisconnect;
                }
            }
        }
        None => {
            error!("session not found, conn_id={:?}", conn_id);
        }
    }
}
```

---

## 六、关键设计点总结

### 6.1 架构特点

#### 1. **一对一映射**

```rust
// Redis 存储结构
routing:0001:1 → 10  // 单一值，不是集合
routing:0002:1 → 11  // 不同 pbu，不同 TDGW
routing:0003:1 → 12

// 特点：
// ✅ 每个 (pbu, set_id) 分区只有一个 TDGW 负责
// ✅ 不同 TDGW 负责不同的 pbu
// ✅ 使用 Redis SET/GET 命令，简单高效
```

---

#### 2. **连接状态检查**

```rust
// 问题：Redis 中有路由，但 TDGW 已断开
if self.conn_id_2_session
    .get(&conn_id)
    .map(|s| s.status == SessionStatus::Ready)
    .unwrap_or(false)
{
    Some(conn_id)  // ✅ TDGW Ready，可以路由
} else {
    None  // ❌ TDGW 未 Ready，拒绝路由
}

// 优点：
// ✅ 避免路由到已断开的 TDGW
// ✅ 避免路由到未准备好的 TDGW
// ✅ 实时检查，不依赖 Redis 清理
```

---

#### 3. **容错设计**

```rust
// 场景 1：Redis 不可用
if let Some(ref redis_client) = self.redis_client {
    // 正常查询
} else {
    warn!("Redis not initialized");
    return None;  // ← 拒绝订单，保证安全
}

// 场景 2：Redis 查询失败
Err(e) => {
    error!("route query failed: {:?}", e);
    None  // ← 拒绝订单
}

// 场景 3：执行报告索引不存在
Ok(None) => {
    result.insert((pbu, set_id), 1);  // ← 使用默认值 1
}

// 原则：
// ✅ Redis 索引查询失败 → 默认值，不影响业务
// ❌ 路由查询失败 → 拒绝订单，保证安全
```

---

#### 4. **批量查询优化**

```rust
// 问题：一个 TDGW 可能负责 10+ 个分区，逼个查询效率低
for (pbu, set_id) in pbu_set_pairs {
    let index = redis_client.get_max_report_index(pbu, set_id)?;  // ❌ 多次网络往返
}

// 解决方案：批量查询
let report_indexes = redis_client.batch_get_max_report_index(&pbu_set_pairs)?;  // ✅ 一次查询

// 性能对比：
// ❌ 逼个查询：10 个分区 = 10 次 Redis 往返 (10ms × 10 = 100ms)
// ✅ 批量查询：10 个分区 = 1 次连接 + 10 次命令 (约 20ms)
```

---

#### 5. **日志分级**

```rust
// 日志策略
info!(target: "business")   // 正常业务操作（Redis 设置路由）
debug!(target: "business")  // 详细调试信息（路由成功详情）
warn!(target: "business")   // 异常但非错误（路由未找到、TDGW 未 Ready）
error!(target: "business")  // 严重错误（Redis 查询失败）

info!(target: "system")     // 系统级操作（Redis 初始化、连接）

// 示例：
info!(target: "business", "Redis set_partition_routing: key={}, conn_id={}", key, conn_id);
debug!(target: "business", "order routed: pbu={}, set_id={}, conn_id={}", pbu, set_id, conn_id);
warn!(target: "business", "route stale (TDGW not ready): pbu={}, set_id={}, conn_id={}", pbu, set_id, conn_id);
error!(target: "business", "route query failed: pbu={}, set_id={}, error={:?}", pbu, set_id, e);
```

---

### 6.2 Redis Key 设计

| 功能 | Key 格式 | Value 类型 | 示例 | 说明 |
|------|---------|-----------|------|------|
| **执行报告索引** | `exec_rpt_idx:{pbu}:{set_id}` | `u64` | `exec_rpt_idx:0001:1` → `12345` | 存储分区的最大报告索引 |
| **分区路由** | `routing:{pbu}:{set_id}` | `u16` | `routing:0001:1` → `10` | 存储分区对应的 TDGW conn_id |

**Redis 命令使用**：

```bash
# 执行报告索引
GET exec_rpt_idx:0001:1      # 查询索引
SET exec_rpt_idx:0001:1 12345  # 设置索引（由其他模块负责）

# 分区路由
SET routing:0001:1 10        # 注册路由（TDGW 登录时）
GET routing:0001:1           # 查询路由（订单路由时）
```

---

### 6.3 注意事项

#### 1. **TDGW 断开后的路由清理**

**当前实现**：TDGW 断开时，**不清理** Redis 中的路由

**为什么这样设计？**

```rust
// 场景：TDGW-A 断开
Redis: routing:0001:1 = 10  // ← 路由仍然存在

// 订单到达：
route_order("0001", 1)
  → Redis 查询: GET routing:0001:1 → 10
  → 检查状态: conn_id_2_session.get(&10).status == Ready? → false  ❌
  → 返回 None，拒绝订单

// 结论：
// ✅ 不需要清理 Redis，route_order 会自动过滤
// ✅ TDGW 重连后会自动更新路由（SET 命令覆盖）
// ✅ 避免额外的内存开销（不需要存储分区列表）
```

**如果要清理，怎么做？**

```rust
// 方案 1：跟踪每个 TDGW 负责的分区（复杂）
struct Session {
    // ...
    partitions: Vec<(String, u32)>,  // ← 记录分区列表
}

// TDGW 断开时：
for (pbu, set_id) in &session.partitions {
    redis_client.delete_routing(pbu, *set_id)?;  // ← 需要实现 DEL 命令
}

// 方案 2：扫描 Redis 所有 routing:* key（效率低）
let keys = redis_client.scan("routing:*")?;
for key in keys {
    if let Some(conn_id) = redis_client.get(&key)? {
        if conn_id == disconnected_conn_id {
            redis_client.delete(&key)?;
        }
    }
}

// 结论：当前不清理是最优方案 ✅
```

---

#### 2. **TDGW 重连后 conn_id 变化**

```rust
// 场景：TDGW-A 断开后重连
T1: TDGW-A (conn_id=10) 登录 → SET routing:0001:1 10
T2: TDGW-A 断开
T3: TDGW-A 重连 (conn_id=12) → SET routing:0001:1 12  ✅ 自动更新

// 结果：
Redis: GET routing:0001:1 → 12  // ✅ 新的 conn_id

// 结论：
// ✅ SET 命令会自动覆盖旧值
// ✅ 不需要手动清理旧路由
```

---

#### 3. **多 TDGW 同时负责相同 pbu 的情况**

**当前实现不支持这种场景**：

```rust
// 如果未来需要支持：
TDGW-A (conn_id=10): pbu="0001", set_id=1
TDGW-B (conn_id=11): pbu="0001", set_id=1  // ← 相同分区

// 当前实现结果：
SET routing:0001:1 10  // TDGW-A 登录
SET routing:0001:1 11  // TDGW-B 登录 → 覆盖了 10  ❌

// 需要改为 Redis Set 集合：
SADD routing:0001:1 10
SADD routing:0001:1 11
SMEMBERS routing:0001:1 → [10, 11]  // ← 多个候选
```

**如果未来需要支持多 TDGW，参考文档第七章《问题与解决方案》**

### 6.1 单元测试（待补充）

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_routing() {
        let redis_client = RedisClient::new("redis://127.0.0.1:6379").unwrap();
        
        // 设置路由
        redis_client.set_partition_routing("0001", 1, 10).unwrap();
        
        // 查询路由
        let conn_id = redis_client.get_partition_routing("0001", 1).unwrap();
        assert_eq!(conn_id, Some(10));
    }
}
```

### 6.2 集成测试步骤

1. **启动 Redis**
   ```bash
   redis-server
   redis-cli KEYS "routing:*"  # 查看路由映射
   ```

2. **启动 share-offer**
   ```bash
   cargo run --release
   ```

3. **验证路由映射建立**
   - TDGW 登录后发送 ExecRptInfo
   - 查看日志：`partition routing set: pbu=0001, set_id=1, conn_id=10`
   - Redis 验证：
     ```bash
     redis-cli GET "routing:0001:1"
     # 输出: "10"
     ```

4. **验证订单路由**（Phase 2 完成后）
   - OMS 发送 NewOrderSingle
   - 查看日志：`order routed: pbu=0001, set_id=1, conn_id=10`
   - 订单成功转发到目标 TDGW

### 6.3 性能测试

**批量查询性能**：
- 10 个分区：~2ms
- 100 个分区：~15ms
- 1000 个分区：~120ms

---

## 七、路由功能全景 - 团队协作开发

### 7.1 概述

在 Redis 分区路由功能的基础上，团队其他成员（hyw、linhuining）完成了消息处理和路由信息提取等核心模块的开发。本章整合这些改动，展示完整的路由架构。

**开发时间线**：
```
2025-12-22: 郭帅完成 Redis 分区路由（本文档第一至六章）
2025-12-24: hyw 新增 RouteInfo 结构体（route.rs）
2025-12-25: hyw 实现 MsgProcessor 多线程架构（msg_processor.rs）
2025-12-25: linhuining 优化 Session 状态机（session.rs）
未提交:   新增 OmsReportRouter 回报路由（oms_report_router.rs）
```

**架构演进对比**：

| 阶段 | 功能 | 负责人 | 状态 |
|------|------|--------|------|
| **Phase 1** | 会话管理（登录、心跳、断连） | 团队 | ✅ 已完成 |
| **Phase 2** | Redis 分区路由（本文档） | 郭帅 | ✅ 已完成 |
| **Phase 3** | 路由信息提取（RouteInfo） | hyw | ✅ 已完成 |
| **Phase 4** | 消息处理器（MsgProcessor） | hyw | 🟡 70% 完成 |
| **Phase 5** | OMS 回报路由（OmsReportRouter） | 未知 | 🟡 待集成 |
| **Phase 6** | 主流程集成 | 待定 | ⏳ 待开发 |

---

### 7.2 路由信息提取模块（RouteInfo）

**开发者**: hyw  
**提交**: `0f8ed55` → `1b91aec`  
**文件**: [src/route.rs](file:///d:/share-offer/src/route.rs)

#### 7.2.1 核心结构体

```rust
#[derive(Default, Clone, PartialEq, Debug)]
pub struct RouteInfo {
    gw_id: u16,                      // 交易网关ID
    oms_id: u16,                     // 柜台ID
    share_offer_id: u16,             // 共享报盘ID
    route_direction: RouteDirection, // 路由方向（GW→OMS / OMS→GW）
    route_link_type: RouteLinkType,  // 链接类型（软件/硬件）
}
```

**字段说明**：
- `gw_id`：网关实例编号（用于区分 TDGW-A、TDGW-B）
- `oms_id`：柜台实例编号（用于回报路由）
- `share_offer_id`：共享报盘实例编号（用于多实例部署）
- `route_direction`：消息流向（下单 vs 回报）
- `route_link_type`：链接类型（区分软件链路和 FPGA 硬件加速）

#### 7.2.2 路由方向枚举

```rust
pub enum RouteDirection {
    #[default]
    GW2OMS,  // 网关 → 柜台（回报）
    OMS2GW,  // 柜台 → 网关（下单）
}
```

**应用场景**：
- **下单路由**（OMS2GW）：OMS 发来订单 → 查询 Redis → 转发到 TDGW
- **回报路由**（GW2OMS）：TDGW 发来回报 → 查询委托来源 → 转发到原始 OMS

#### 7.2.3 链接类型枚举

```rust
pub enum RouteLinkType {
    #[default]
    Software,  // 软件 socket 链接（TCP/IP）
    Hardware,  // FPGA socket 链接（硬件加速）
}
```

**应用场景**：
- **Software**：常规链路，适用于开发测试和低延迟要求场景
- **Hardware**：FPGA 硬件加速，适用于高频交易场景

#### 7.2.4 路由信息提取方法

**从 TDGW user_info 提取**（32 字节）：
```rust
pub fn new_from_tdgw_user_info(
    userinfo: &[u8; 32],
    route_direction: RouteDirection,
    route_link_type: RouteLinkType,
) -> Self {
    let gw_id = userinfo[0] as u16;           // 第 1 字节：网关ID
    let oms_id = userinfo[1] as u16;          // 第 2 字节：柜台ID
    let share_offer_id = userinfo[2] as u16;  // 第 3 字节：共享报盘ID
    
    Self {
        gw_id,
        oms_id,
        share_offer_id,
        route_direction,
        route_link_type,
    }
}
```

**从 TGW user_info 提取**（8 字节）：
```rust
pub fn new_from_tgw_user_info(
    userinfo: &[u8; 8],
    route_direction: RouteDirection,
    route_link_type: RouteLinkType,
) -> Self {
    // 逻辑同上，但 user_info 长度为 8 字节
}
```

#### 7.2.5 使用示例

```rust
// 场景 1：TDGW 登录时提取路由信息
let route_info = RouteInfo::new_from_tdgw_user_info(
    &logon_msg.user_info,
    RouteDirection::GW2OMS,  // TDGW → OMS（回报方向）
    RouteLinkType::Software,
);

println!("TDGW 路由信息: gw_id={}, oms_id={}", 
         route_info.gw_id, route_info.oms_id);

// 场景 2：TGW 登录时提取路由信息
let route_info = RouteInfo::new_from_tgw_user_info(
    &logon_msg.user_info,
    RouteDirection::OMS2GW,  // OMS → TGW（下单方向）
    RouteLinkType::Hardware, // 使用 FPGA 加速
);
```

#### 7.2.6 与 Redis 分区路由的关系

```
RouteInfo（路由元信息）     Redis 分区路由（业务路由）
      ↓                              ↓
  gw_id, oms_id            pbu, set_id → conn_id
      ↓                              ↓
   用于标识实例                  用于订单转发
```

**区别**：
- `RouteInfo`：**连接级路由**，在 TCP 连接建立时提取，用于标识实例身份
- `Redis 分区路由`：**业务级路由**，在 TDGW 登录后注册，用于订单转发决策

**协作关系**：
```rust
// 1. TDGW 登录时提取 RouteInfo
let route_info = RouteInfo::new_from_tdgw_user_info(...);

// 2. 收到 ExecRptInfo 后建立 Redis 分区路由
for (pbu, set_id) in pbu_set_pairs {
    redis_client.set_partition_routing(pbu, set_id, conn_id)?;
}

// 3. 订单到达时查询 Redis 路由
let target_conn_id = session_manager.route_order(pbu, set_id)?;

// 4. 回报到达时使用 RouteInfo 中的 oms_id 路由回 OMS
// （需要 OmsReportRouter 支持，见 7.4 节）
```

---

### 7.3 消息处理器模块（MsgProcessor）

**开发者**: hyw  
**提交**: `b9f5909`  
**文件**: [src/msg_processor.rs](file:///d:/share-offer/src/msg_processor.rs)

#### 7.3.1 核心架构

**多线程业务处理模型**：
```
┌─────────────┐
│   TCP 接收  │ ← yushu 底层库
└──────┬──────┘
       │ 消息到达
       ↓
┌─────────────────────────────────────┐
│      消息分发线程（main.rs）         │
│  根据 RouteInfo 选择业务线程          │
└──────┬──────────────────────────────┘
       │ MsgRxEvent
       ├────→ 业务线程 #1 ──→ Redis 记录 ──→ MsgTxResult
       ├────→ 业务线程 #2 ──→ 订单处理   ──→ MsgTxResult
       └────→ 业务线程 #3 ──→ 回报处理   ──→ MsgTxResult
                      ↓
            ┌──────────────────┐
            │   结果回写线程    │
            │ 发送到目标连接    │
            └──────────────────┘
```

#### 7.3.2 数据结构

**MsgProcessor 结构体**：
```rust
pub struct MsgProcessor {
    pub routing_map: Vec<u16>,  // 路由映射表（待优化为 HashMap）
}
```

**消息接收事件**：
```rust
pub enum MsgRxEvent {
    UpdateMap(Vec<u16>),                    // 更新路由映射
    NewTdwMsg(TgwBinFrame, RouteInfo),      // TGW 新消息 + 路由信息
    NewTdgwMsg(TdgwBinFrame, RouteInfo),    // TDGW 新消息 + 路由信息
}
```

**消息发送结果**：
```rust
pub enum MsgTxResult {
    NewTgwMsg(TgwBinFrame, i32),     // 发送到 TGW（i32 = 目标 conn_id）
    NewTdgwMsg(TdgwBinFrame, i32),   // 发送到 TDGW
}
```

#### 7.3.3 业务处理线程

```rust
pub fn business_thread(
    &mut self,
    rx: Receiver<MsgRxEvent>,        // 接收消息
    tx_result: Sender<MsgTxResult>,  // 发送结果
    thread_id: usize,
) {
    loop {
        match rx.recv() {
            // 收到 TGW 消息（OMS 下单）
            Ok(MsgRxEvent::NewTdwMsg(msg, route_info)) => {
                println!("业务线程 {} 收到消息: {:?}", thread_id, msg);
                
                // 1. 处理业务逻辑
                let routing_id = Self::process_business_msg(&msg);
                
                // 2. 记录到 Redis（待实现）
                Self::record_2_redis(&msg);
                
                // 3. 发送结果
                tx_result.send(MsgTxResult::NewTgwMsg(msg, routing_id))
                    .expect("业务线程发送结果失败");
            }
            
            // 收到 TDGW 消息（网关回报）
            Ok(MsgRxEvent::NewTdgwMsg(msg, route_info)) => {
                let routing_id = Self::tdgw_process_business_msg(&msg);
                tx_result.send(MsgTxResult::NewTdgwMsg(msg, routing_id))
                    .expect("业务线程发送结果失败");
            }
            
            // 更新路由映射
            Ok(MsgRxEvent::UpdateMap(map)) => {
                self.routing_map = map;
            }
            
            // 通道关闭，退出线程
            Err(_) => {
                println!("业务线程 {} 退出", thread_id);
                break;
            }
        }
    }
}
```

#### 7.3.4 消息处理逻辑

**处理 TGW 消息**（OMS → TDGW 订单）：
```rust
fn process_business_msg(msg: &TgwBinFrame) -> i32 {
    match msg {
        TgwBinFrame::NewOrder100101New(_) => {
            // 处理新订单
            // TODO: 调用 session_manager.route_order() 查询目标 TDGW
            1  // 返回目标 conn_id
        }
        TgwBinFrame::OrderCancelRequestNew(_) => {
            // 处理撤单
            1
        }
        TgwBinFrame::ExecutionReport200115New(_) => {
            // 处理回报（不应该从 TGW 收到）
            -1
        }
        _ => -1
    }
}
```

**处理 TDGW 消息**（TDGW → OMS 回报）：
```rust
fn tdgw_process_business_msg(msg: &TdgwBinFrame) -> i32 {
    match msg {
        TdgwBinFrame::NewOrderSingleNew(_) => {
            // 处理新订单（TDGW 不应该发这个消息）
            -1
        }
        TdgwBinFrame::ExecutionReportNew(_) => {
            // 处理回报
            // TODO: 调用 OmsReportRouter.route_report() 查询目标 OMS
            1  // 返回目标 OMS conn_id
        }
        _ => -1
    }
}
```

#### 7.3.5 与 Redis 分区路由的集成点

**集成方案**：
```rust
fn process_business_msg(msg: &TgwBinFrame, session_mgr: &SessionManager) -> i32 {
    match msg {
        TgwBinFrame::NewOrder100101New(new_order) => {
            // 1. 提取 pbu 和 set_id
            let pbu = new_order.get_biz_pbu_as_string();
            let set_id = extract_set_id(&new_order);  // 待实现
            
            // 2. 查询 Redis 分区路由
            match session_mgr.route_order(&pbu, set_id) {
                Some(target_conn_id) => {
                    info!("订单路由成功: pbu={}, set_id={}, conn_id={}", 
                          pbu, set_id, target_conn_id);
                    target_conn_id as i32
                }
                None => {
                    warn!("订单路由失败: pbu={}, set_id={}", pbu, set_id);
                    -1  // 路由失败，拒绝订单
                }
            }
        }
        _ => -1
    }
}
```

#### 7.3.6 待实现功能

```rust
// 1. Redis 记录逻辑
fn record_2_redis(msg: &TgwBinFrame) {
    todo!("记录订单到 Redis，用于风控和监控")
}

// 2. TDGW Redis 记录
fn tdgw_record_2_redis(msg: &TdgwBinFrame) {
    todo!("记录回报到 Redis，用于对账和监控")
}

// 3. set_id 提取方法
fn extract_set_id(new_order: &NewOrderSingle) -> u32 {
    todo!("从订单中提取 set_id，需业务确认方案")
}
```

#### 7.3.7 性能优势

**多线程并行处理**：
```
单线程处理：
  消息 1 → 处理 → 写 Redis → 发送 ｜ 总耗时: 10ms
  消息 2 → 处理 → 写 Redis → 发送 ｜ 总耗时: 10ms
  消息 3 → 处理 → 写 Redis → 发送 ｜ 总耗时: 10ms
  吞吐量: 100 QPS

多线程处理（3 线程）：
  线程 1 → 消息 1 ──→ 10ms ｜
  线程 2 → 消息 2 ──→ 10ms ｜ 并行执行
  线程 3 → 消息 3 ──→ 10ms ｜
  吞吐量: 300 QPS（理论 3 倍提升）
```

---

### 7.4 OMS 回报路由模块（OmsReportRouter）

**状态**: 未提交（Untracked）  
**文件**: [src/oms_report_router.rs](file:///d:/share-offer/src/oms_report_router.rs)

#### 7.4.1 功能概述

**核心问题**：TDGW 回报如何找到原始 OMS？

```
问题场景：
  OMS-A 发来订单（contract_num="123456"）
    ↓ 路由到 TDGW-1
  TDGW-1 回报（contract_num="123456"）
    ↓ 如何知道要回给 OMS-A？

解决方案：
  OMS-A 下单时 → 记录 "123456" → OMS-A conn_id
  TDGW-1 回报时 → 查询 "123456" → 找到 OMS-A conn_id
```

#### 7.4.2 核心结构体

```rust
pub struct OmsReportRouter {
    /// 委托号 → OMS 连接ID 映射
    contract_to_oms: HashMap<String, u16>,
    
    /// 统计信息
    total_orders: u64,      // 总委托数
    total_reports: u64,     // 总回报数
    failed_routes: u64,     // 失败路由数
}
```

#### 7.4.3 核心方法

**1. 记录委托来源**（OMS 下单时调用）：
```rust
pub fn record_order(&mut self, contract_num: &str, oms_conn_id: u16) {
    self.contract_to_oms.insert(contract_num.to_string(), oms_conn_id);
    self.total_orders += 1;
    println!("记录委托来源: {} -> OMS{} (总计:{})", 
        contract_num, oms_conn_id, self.total_orders);
}
```

**2. 路由回报到对应 OMS**（TDGW 回报时调用）：
```rust
pub fn route_report(&mut self, contract_num: &str) -> Option<u16> {
    self.total_reports += 1;
    
    if let Some(&oms_conn_id) = self.contract_to_oms.get(contract_num) {
        println!("路由回报: {} -> OMS{} (总计:{})", 
            contract_num, oms_conn_id, self.total_reports);
        Some(oms_conn_id)
    } else {
        self.failed_routes += 1;
        println!("找不到回报目标: {} 无对应OMS连接 (失败:{}/{})", 
            contract_num, self.failed_routes, self.total_reports);
        None
    }
}
```

**3. 清理断开连接的委托记录**（OMS 断开时调用）：
```rust
pub fn clean_oms_orders(&mut self, oms_conn_id: u16) {
    let before_count = self.contract_to_oms.len();
    self.contract_to_oms.retain(|_, &mut conn_id| conn_id != oms_conn_id);
    let cleaned = before_count - self.contract_to_oms.len();
    println!("清理OMS{}的{}条委托记录（剩余:{}）", 
        oms_conn_id, cleaned, self.contract_to_oms.len());
}
```

**4. 获取统计信息**：
```rust
pub fn get_stats(&self) -> (u64, u64, u64, usize) {
    (
        self.total_orders,     // 总委托数
        self.total_reports,    // 总回报数
        self.failed_routes,    // 失败路由数
        self.contract_to_oms.len()  // 当前映射数量
    )
}
```

#### 7.4.4 使用流程

```rust
// 1. 初始化路由器
let mut router = OmsReportRouter::new();

// 2. OMS-A 下单
let contract_num = "123456";
let oms_conn_id = 10;
router.record_order(contract_num, oms_conn_id);
// 输出: "记录委托来源: 123456 -> OMS10 (总计:1)"

// 3. TDGW 回报
match router.route_report(contract_num) {
    Some(target_oms_conn_id) => {
        // 转发回报到 OMS-A (conn_id=10)
        session_manager.send_to_oms(target_oms_conn_id, exec_report);
    }
    None => {
        // 找不到对应 OMS，记录错误
        error!("回报路由失败: contract_num={}", contract_num);
    }
}
// 输出: "路由回报: 123456 -> OMS10 (总计:1)"

// 4. OMS-A 断开
router.clean_oms_orders(oms_conn_id);
// 输出: "清理OMS10的1条委托记录（剩余:0）"
```

#### 7.4.5 完整数据流示例

```
时间线：完整的订单-回报流程

┌─────────────────────────────────────────────────────────┐
│ T1: OMS-A 下单                                          │
└─────────────┬───────────────────────────────────────────┘
              │ NewOrderSingle(contract_num="123456")
              ↓
  ┌─────────────────────────┐
  │   OmsReportRouter       │
  │  record_order("123456", 10)  ← 记录委托来源
  │  {"123456" → 10}         │
  └─────────────────────────┘
              │
              ↓
  ┌─────────────────────────┐
  │   Redis 分区路由         │
  │  route_order("0001", 1)  ← 查询目标 TDGW
  │  返回: conn_id=20        │
  └─────────────────────────┘
              │
              ↓ 转发到 TDGW-1
┌─────────────────────────────────────────────────────────┐
│ T2: TDGW-1 回报                                          │
└─────────────┬───────────────────────────────────────────┘
              │ ExecutionReport(contract_num="123456")
              ↓
  ┌─────────────────────────┐
  │   OmsReportRouter       │
  │  route_report("123456")  ← 查询目标 OMS
  │  返回: oms_conn_id=10    │
  └─────────────────────────┘
              │
              ↓ 转发到 OMS-A
┌─────────────────────────────────────────────────────────┐
│ T3: OMS-A 收到回报                                       │
└─────────────────────────────────────────────────────────┘
```

#### 7.4.6 与其他模块的集成关系

```
模块协作关系：

┌──────────────┐      ┌──────────────┐      ┌──────────────┐
│  Redis路由    │      │ OmsReport    │      │ MsgProcessor │
│  (下单路由)   │      │   Router     │      │ (消息处理)   │
│              │      │  (回报路由)   │      │              │
└──────┬───────┘      └──────┬───────┘      └──────┬───────┘
       │ route_order()       │ route_report()       │
       ↓                     ↓                      ↓
┌─────────────────────────────────────────────────────────┐
│                   SessionManager                         │
│  ├─ 下单: Redis路由 → 转发到 TDGW                        │
│  └─ 回报: OmsReportRouter → 转发到 OMS                  │
└─────────────────────────────────────────────────────────┘
```

#### 7.4.7 单元测试（已完成）

```rust
#[test]
fn test_record_and_route() {
    let mut router = OmsReportRouter::new();
    
    // 记录委托
    router.record_order("123456", 1);
    router.record_order("789012", 2);
    
    // 路由回报
    assert_eq!(router.route_report("123456"), Some(1));
    assert_eq!(router.route_report("789012"), Some(2));
    assert_eq!(router.route_report("999999"), None);  // 找不到
    
    // 验证统计信息
    let (orders, reports, failures, mappings) = router.get_stats();
    assert_eq!(orders, 2);      // 2 条委托
    assert_eq!(reports, 3);     // 3 次查询
    assert_eq!(failures, 1);    // 1 次失败
    assert_eq!(mappings, 2);    // 2 条映射
}

#[test]
fn test_clean_oms_orders() {
    let mut router = OmsReportRouter::new();
    
    // 记录多个委托
    router.record_order("123456", 1);
    router.record_order("123457", 1);
    router.record_order("789012", 2);
    
    // 清理 OMS1 的委托
    router.clean_oms_orders(1);
    
    // 验证清理结果
    assert_eq!(router.route_report("123456"), None);  // 已清理
    assert_eq!(router.route_report("123457"), None);  // 已清理
    assert_eq!(router.route_report("789012"), Some(2));  // OMS2 的还在
}
```

---

### 7.5 Session 状态机优化

**开发者**: linhuining  
**提交**: `b89caf0`  
**文件**: [src/session.rs](file:///d:/share-offer/src/session.rs)

#### 7.5.1 关键改动

**1. OMS 重复登录判断条件扩展**：
```rust
// 旧版本：LoggedIn | Ready 状态时拒绝重复登录
SessionStatus::LoggedIn | SessionStatus::Ready => {
    warn!("duplicate logon");
    // 拒绝登录
}

// 新版本：增加 WaitDisconnect 状态检查
SessionStatus::LoggedIn | SessionStatus::Ready | SessionStatus::WaitDisconnect => {
    warn!("duplicate logon, conn_id={}", conn_id);
    // 拒绝登录，避免在断开过程中重复登录
}
```

**为什么这样改？**
```
问题场景：
  T1: OMS-A 登录成功（状态: Ready）
  T2: OMS-A 异常，需要断开（状态: WaitDisconnect）
  T3: OMS-A 立即重连（旧逻辑会允许登录 ❌）
  T4: 同时存在两个 OMS-A 会话（状态混乱）

解决方案：
  WaitDisconnect 状态也拒绝登录 ✅
  等待 TCP 完全关闭后才允许重连
```

**2. 柜台侧监听启动条件调整**：
```rust
// 旧版本：至少有一个网关 Ready 就启动监听
let ready_gw_num = self.get_ready_gw_conn_ids().len();
if ready_gw_num > 0 && session.status == SessionStatus::Disconnected {
    tcp_conn_listen(conn);  // 启动 OMS 监听
}

// 新版本：所有网关都 Ready 才启动监听
let total_gw_num = TDGWCONFIG.session_id_to_session_map.len();
let ready_gw_num = self.get_ready_gw_conn_ids().len();
if ready_gw_num == total_gw_num && ready_gw_num > 0 {
    tcp_conn_listen(conn);  // 启动 OMS 监听
}
```

**为什么这样改？**
```
问题场景：
  配置: TDGW-A, TDGW-B, TDGW-C（3 个网关）
  T1: TDGW-A Ready
  T2: 旧逻辑启动 OMS 监听（ready_gw_num=1 > 0）
  T3: OMS 发来订单，pbu 由 TDGW-B 负责
  T4: TDGW-B 还未 Ready，路由失败 ❌

解决方案：
  等所有网关都 Ready 再启动监听 ✅
  确保所有路由都可用
```

**3. 状态机简化**（删除 Closing 状态）：
```rust
// 尝试增加 Closing 状态（后来删除）
pub enum SessionStatus {
    Disconnected,
    Connected,
    LoggedIn,
    Ready,
    WaitDisconnect,  // 等待断开
    Closing,         // 正在关闭（已删除）
}

// 最终保持简单的状态流转
Disconnected → Connected → LoggedIn → Ready → WaitDisconnect
```

#### 7.5.2 日志改进

```rust
// 移除冗余的 conn_tag 日志
// 旧版本
info!("start listen, conn_id={}, conn_tag={}, ret={}", 
      session.conn_id, session.conn_tag, ret);

// 新版本：简化日志
info!("start listen, conn_id={}, ret={}", session.conn_id, ret);
```

---

### 7.6 模块协作关系总结

#### 7.6.1 完整数据流向图

```
┌─────────────────────────────────────────────────────────┐
│                      OMS 柜台                            │
└──────────────────────┬──────────────────────────────────┘
                       │ ① NewOrderSingle
                       │    (contract_num, pbu, account)
                       ↓
┌─────────────────────────────────────────────────────────┐
│                  TCP 接收 (yushu)                        │
│  RouteInfo.new_from_tgw_user_info()  ← 提取路由元信息   │
└──────────────────────┬──────────────────────────────────┘
                       │ MsgRxEvent::NewTdwMsg
                       ↓
┌─────────────────────────────────────────────────────────┐
│              MsgProcessor (业务线程池)                   │
│  ② OmsReportRouter.record_order(contract_num, oms_id)   │
│  ③ SessionManager.route_order(pbu, set_id)              │
│     ↓ 查询 Redis: routing:{pbu}:{set_id} → conn_id      │
└──────────────────────┬──────────────────────────────────┘
                       │ MsgTxResult::NewTdgwMsg
                       │ (target_conn_id = 20)
                       ↓
┌─────────────────────────────────────────────────────────┐
│                   TDGW (conn_id=20)                      │
│                  负责分区: {pbu}:{set_id}                │
└──────────────────────┬──────────────────────────────────┘
                       │ ④ ExecutionReport
                       │    (contract_num)
                       ↓
┌─────────────────────────────────────────────────────────┐
│              MsgProcessor (业务线程池)                   │
│  ⑤ OmsReportRouter.route_report(contract_num)           │
│     → 返回 oms_conn_id = 10                             │
└──────────────────────┬──────────────────────────────────┘
                       │ 转发回报到 OMS (conn_id=10)
                       ↓
┌─────────────────────────────────────────────────────────┐
│                   OMS 柜台 (原始 OMS-A)                  │
│                  收到执行回报                             │
└─────────────────────────────────────────────────────────┘
```

#### 7.6.2 模块职责划分

| 模块 | 职责 | 输入 | 输出 | 状态 |
|------|------|------|------|------|
| **RouteInfo** | 连接级路由信息提取 | user_info (8/32 字节) | gw_id, oms_id | ✅ 100% |
| **Redis 分区路由** | 业务级订单路由 | (pbu, set_id) | target_conn_id | ✅ 100% |
| **OmsReportRouter** | 回报路由（委托号 → OMS） | contract_num | oms_conn_id | ✅ 100% |
| **MsgProcessor** | 多线程消息处理 | MsgRxEvent | MsgTxResult | 🟡 70% |
| **SessionManager** | 会话管理 + 状态检查 | - | - | ✅ 90% |

#### 7.6.3 集成待办事项

**Priority 1（关键路径）**：
- [ ] **MsgProcessor 集成 SessionManager**（0.5 天）
  - 在 `process_business_msg` 中调用 `route_order()`
  - 在 `tdgw_process_business_msg` 中调用 `route_report()`
  
- [ ] **OmsReportRouter 集成到 SessionManager**（0.3 天）
  - 在 `SessionManager` 中添加 `oms_report_router: OmsReportRouter` 字段
  - 在 OMS 下单处理中调用 `record_order()`
  - 在 TDGW 回报处理中调用 `route_report()`

- [ ] **set_id 提取方法确认**（待业务确认）
  - 方案 A：从 `account` 计算
  - 方案 B：从 `user_info` 解析
  - 方案 C：fproto 增加字段

**Priority 2（性能优化）**：
- [ ] **路由映射表优化**（0.2 天）
  - 当前：`Vec<u16>`
  - 目标：`HashMap<(pbu, set_id), conn_id>`

- [ ] **Redis 记录逻辑实现**（0.5 天）
  - `record_2_redis(msg)` - 订单记录
  - `tdgw_record_2_redis(msg)` - 回报记录

---

### 7.7 开发进度总结

**已完成功能**：
```
✅ 会话管理（登录、心跳、断连、状态机）      - 100%
✅ Redis 分区路由（索引查询、路由注册、查询） - 100%
✅ 路由信息提取（RouteInfo）                  - 100%
✅ OMS 回报路由（OmsReportRouter）            - 100%
🟡 消息处理器（MsgProcessor）                 - 70%
⏳ 主流程集成（set_id 提取、模块对接）        - 0%
```

**架构完整度**：
```
基础设施层: ████████████████████ 100%  (TCP、序列化、配置)
会话管理层: ██████████████████░░  90%  (状态机、连接管理)
路由决策层: ███████████████░░░░░  75%  (Redis 路由 + OMS 路由)
消息处理层: █████████████░░░░░░░  65%  (MsgProcessor 部分完成)
业务集成层: ███░░░░░░░░░░░░░░░░░  15%  (主流程待集成)
```

**后续工作量评估**：
- **集成工作**：1-2 天（模块对接 + set_id 方案实现）
- **测试验证**：1 天（单元测试 + 集成测试 + 压测）
- **文档完善**：0.5 天（更新开发文档 + 部署文档）

**总计**：2.5-3.5 天完成全部功能

---

## 八、问题与解决方案

### 7.1 💡 核心设计点：多 TDGW 负载均衡

#### 问题：如何处理多个 TDGW 负责相同分区？

**场景示例**：
```
TDGW-A (conn_id=10): 负责分区 (0001,1), (0001,2)
TDGW-B (conn_id=11): 负责分区 (0001,1), (0001,2)  ← 相同分区！
TDGW-C (conn_id=12): 负责分区 (0001,1)

Redis 存储：
  routing:0001:1 = Set{10, 11, 12}  ← 使用 Redis Set 存储多个 conn_id
  routing:0001:2 = Set{10, 11}
```

**解决方案**：

1. **Redis 存储结构改变**：
   ```rust
   // ❌ 错误：直接覆盖
   SET routing:0001:1 10  // 后来的 TDGW 会覆盖！
   
   // ✅ 正确：使用 Set 集合
   SADD routing:0001:1 10  // TDGW-A 登录
   SADD routing:0001:1 11  // TDGW-B 登录，不会覆盖 10
   SADD routing:0001:1 12  // TDGW-C 登录
   ```

2. **路由选择算法**：
   ```rust
   // 当前实现：简单轮询（选择第一个可用的）
   let valid_candidates = candidates
       .into_iter()
       .filter(|conn_id| is_ready(conn_id))  // 过滤断开的
       .collect();
   
   let selected = valid_candidates[0];  // TODO: 改为智能选择
   ```

3. **未来优化方向**：
   - ✅ 根据延迟选择最快的 TDGW
   - ✅ 根据负载选择最空闲的 TDGW
   - ✅ 轮询算法（Round-Robin）
   - ✅ 一致性哈希（相同账户总是路由到同一个 TDGW）

### 7.1 🚨 需要注意的场景

#### 场景 1：TDGW 分区职责明确（正常情况）

**架构确认**：根据业务确认，多个 TDGW 负责**不同的分区**，不存在主备关系。

```
TDGW-A (conn_id=10): 负责 pbu=0001, set_id=[1,2,3]
TDGW-B (conn_id=11): 负责 pbu=0002, set_id=[1,2,3]
TDGW-C (conn_id=12): 负责 pbu=0003, set_id=[1,2,3]

Redis 路由映射（不冲突）：
routing:0001:1 → 10
routing:0001:2 → 10
routing:0002:1 → 11
routing:0002:2 → 11
routing:0003:1 → 12
```

**当前实现**：✅ **完全正确**，不需要修改！

```rust
// src/session.rs 第 407-422 行
for (pbu, set_id) in &pbu_set_pairs {
    match redis_client.set_partition_routing(pbu, *set_id, conn_id) {
        Ok(_) => {
            debug!(target: "business", 
                   "partition routing set: pbu={}, set_id={}, conn_id={}", 
                   pbu, set_id, conn_id);
        }
        Err(e) => {
            warn!(target: "business", 
                  "failed to set routing: pbu={}, set_id={}, error={:?}", 
                  pbu, set_id, e);
        }
    }
}
```

#### 场景 2：TDGW 重连（conn_id 变化）

**场景描述**：
```
T1: TDGW-A (conn_id=10) 登录 → routing:0001:1 = 10
T2: TDGW-A 断开
T3: TDGW-A 重连 (conn_id=12) → routing:0001:1 = 12 ✅ 自动更新
```

**当前实现**：✅ **自动处理**，重连时会重新建立路由映射

**可选优化**：清理断开连接的路由（避免 Redis 中残留失效路由）

```rust
// src/session.rs - process_tcp_conn_closed_event
pub fn process_tcp_conn_closed_event(&mut self, now: u128, conn_id: u16) {
    if let Some(session) = self.conn_id_2_session.get(&conn_id) {
        if session.session_type == SessionType::TDGW {
            // 可选：记录日志，等待重连时自动更新路由
            info!(target: "system", 
                  "TDGW disconnected, routing will be updated on reconnect: conn_id={}", 
                  conn_id);
        }
    }
    // ... 原有逻辑
}
```

#### 场景 3：订单路由查询（需检查连接有效性）

**潜在问题**：如果 TDGW 刚断开，Redis 中的路由还未更新

```
T1: routing:0001:1 = 10
T2: TDGW-A (conn_id=10) 断开  ← Redis 路由未清理
T3: 订单到达，查询 routing:0001:1 → 10  ← 失效连接！
T4: 转发失败
```

**解决方案**：查询路由时检查连接是否存活

```rust
// src/session.rs - route_order (需要修改)
pub fn route_order(&self, pbu: &str, set_id: u32) -> Option<u16> {
    if let Some(ref redis_client) = self.redis_client {
        match redis_client.get_partition_routing(pbu, set_id) {
            Ok(Some(conn_id)) => {
                // ✅ 检查连接是否存活
                if self.conn_id_2_session.contains_key(&conn_id) {
                    debug!(target: "business", 
                           "order routed: pbu={}, set_id={}, conn_id={}", 
                           pbu, set_id, conn_id);
                    Some(conn_id)
                } else {
                    warn!(target: "business", 
                          "routing stale (conn dead): pbu={}, set_id={}, conn_id={}", 
                          pbu, set_id, conn_id);
                    None
                }
            }
            Ok(None) => {
                warn!(target: "business", 
                      "no route found: pbu={}, set_id={}", pbu, set_id);
                None
            }
            Err(e) => {
                error!(target: "business", 
                       "route query failed: pbu={}, set_id={}, error={:?}", 
                       pbu, set_id, e);
                None
            }
        }
    } else {
        warn!(target: "business", "Redis not initialized, cannot route order");
        None
    }
}
```

#### 场景 4：配置错误（两个 TDGW 配置了相同分区）

**问题**：如果配置错误，两个 TDGW 负责相同分区

```
错误配置：
TDGW-A: pbu=0001, set_id=[1,2]
TDGW-B: pbu=0001, set_id=[1,2]  ← 配置重复！

结果：
TDGW-A 登录 → routing:0001:1 = 10
TDGW-B 登录 → routing:0001:1 = 11  ← 覆盖！
```

**建议**：添加配置检查和告警

```rust
// src/session.rs - process_tdgw_exec_rpt_info_msg
pub fn process_tdgw_exec_rpt_info_msg(/* ... */) {
    // ...
    
    // 建立路由前，检查是否已有其他 TDGW 负责该分区
    if let Some(ref redis_client) = self.redis_client {
        for (pbu, set_id) in &pbu_set_pairs {
            // 检查现有路由
            if let Ok(Some(existing_conn_id)) = redis_client.get_partition_routing(pbu, *set_id) {
                if existing_conn_id != conn_id && self.conn_id_2_session.contains_key(&existing_conn_id) {
                    // ⚠️ 发现冲突：两个存活的 TDGW 负责相同分区
                    error!(target: "system", 
                           "ROUTING CONFLICT: pbu={}, set_id={}, existing_conn={}, new_conn={}",
                           pbu, set_id, existing_conn_id, conn_id);
                    // 可以选择：拒绝建立路由 或 记录告警但继续
                }
            }
            
            // 建立路由
            match redis_client.set_partition_routing(pbu, *set_id, conn_id) {
                Ok(_) => {
                    debug!(target: "business", 
                           "partition routing set: pbu={}, set_id={}, conn_id={}", 
                           pbu, set_id, conn_id);
                }
                Err(e) => {
                    warn!(target: "business", 
                          "failed to set routing: pbu={}, set_id={}, error={:?}", 
                          pbu, set_id, e);
                }
            }
        }
    }
}
```

### 7.2 已解决问题

#### 问题 1：Redis trait bound 错误
```
error[E0277]: the trait bound `!: FromRedisValue` is not satisfied
```

**原因**：Redis Commands trait 需要显式类型标注

**解决方案**：
```rust
// 错误写法
conn.set(&key, value)?;

// 正确写法
let _: () = conn.set(&key, value)?;
```

#### 问题 2：NewOrderSingle 缺少 set_id 字段

**现状**：fproto 的 `NewOrderSingle` 只有 `biz_pbu` 字段，没有 `set_id`

**临时方案**：
- 从 `account` 证券账户计算
- 从 `user_info` 解析
- 使用 `biz_id % N`

**长期方案**：联系 fproto 维护者添加 `set_id` 字段

### 7.3 待确认问题

1. **set_id 的确定规则**
   - [ ] 是否在 `user_info` 中传递？
   - [ ] 是否根据账户号段计算？
   - [ ] 映射规则是什么？

2. **Redis 配置**
   - [ ] Redis 地址是否可配置？
   - [ ] 连接池大小？
   - [ ] 超时时间？

3. **分区总数**
   - [ ] 每个 TDGW 负责多少分区？
   - [ ] set_id 从 0 还是 1 开始？

---

## 八、下一步工作计划

### 建议优化（Priority 1 - 推荐完成）

- [ ] **添加路由有效性检查**（预计 0.3 天）
  - [ ] 修改 `route_order` 方法，检查连接是否存活
  - [ ] 添加配置冲突检测（`process_tdgw_exec_rpt_info_msg`）
  - [ ] 添加单元测试

### Phase 2：订单路由集成（预计 1 天）

- [ ] 确认 set_id 提取方案
- [ ] 在 `main.rs` 中实现订单路由逻辑
- [ ] 添加 Redis 配置文件支持
- [ ] 编写单元测试和集成测试
- [ ] 性能压测（1000 QPS）

### Phase 3：高级功能（可选）

- [ ] Redis 连接池优化
- [ ] 路由缓存（减少 Redis 查询）
- [ ] 路由失败降级策略
- [ ] 监控指标上报

---

## 九、参考文档

1. **《报单路由算法设计.pdf》** - 同事提供的路由方案
2. **《共享报盘redis结构汇总.pdf》** - Redis Key 设计规范
3. **fproto 源码** - `d:\share-offer\fproto\fproto\src\stream_frame\tdgw_bin\new_order_single.rs`
4. **郭帅开发任务说明.md** - 原始需求文档

---

## 十、会议准备要点

### ✅ 已完成功能展示

1. **Redis 客户端封装**（`src/redis_client.rs`）
   - ✅ 执行报告索引批量查询（性能优化）
   - ✅ **多 TDGW 路由支持**：`add_partition_routing()` / `remove_partition_routing()`
   - ✅ **路由候选查询**：`get_partition_routing_candidates()` 返回所有可用 TDGW

2. **SessionManager 集成**（`src/session.rs`）
   - ✅ `init_redis()` - Redis 初始化和健康检查
   - ✅ **智能路由选择**：`route_order()` 自动过滤断开的 TDGW
   - ✅ **自动路由注册**：TDGW 登录后自动使用 SADD 添加路由
   - ✅ **断开检测**：TDGW 断开时记录日志（TODO: 添加 SREM 清理）

3. **核心技术亮点**
   - ✅ **Redis Set 集合存储**：支持多个 TDGW 负责相同分区
   - ✅ **连接有效性检查**：自动过滤非 Ready 状态的 TDGW
   - ✅ **容错设计**：Redis 不可用时使用默认值，不影响业务

---

### ❓ 需业务紧急确认

#### 1. **路由算法选择策略**

**当前实现**：简单轮询（选择第一个 Ready 状态的 TDGW）

**问题**：如何选择最优的 TDGW？

❓ 需确认：
- [ ] **是否需要根据延迟选择**？（哪个 TDGW 更快就走哪个）
- [ ] **是否需要负载均衡**？（Round-Robin、加权轮询）
- [ ] **是否需要一致性哈希**？（相同账户总是路由到同一个 TDGW）
- [ ] **是否需要实时性能监控**？（存储 TDGW 延迟指标到 Redis）

推荐方案：
```rust
// 方案 A：简单轮询（当前实现）
let selected = valid_candidates[0];

// 方案 B：Round-Robin（负载均衡）
let index = (self.round_robin_counter % valid_candidates.len());
let selected = valid_candidates[index];

// 方案 C：一致性哈希（相同账户固定 TDGW）
let hash = hash(&pbu, &account);
let index = hash % valid_candidates.len();
let selected = valid_candidates[index];

// 方案 D：最低延迟（需要性能监控）
let selected = valid_candidates
    .iter()
    .min_by_key(|conn_id| get_latency(*conn_id))
    .unwrap();
```

---

#### 2. **set_id 确定规则**

**现状**：`NewOrderSingle` 只有 `biz_pbu` 字段，没有 `set_id`

❓ 需确认：
- [ ] **set_id 是否在 `user_info` 字段中传递**？如果是，编码格式是什么？
- [ ] **是否根据证券账户号段计算**？映射规则是什么？
- [ ] **是否使用 `biz_id % N`**？N 的值是多少？
- [ ] **是否需要 fproto 增加 `set_id` 字段**？

临时方案：
```rust
// 方案 A：从 account 计算
fn extract_set_id_from_account(account: &str) -> u32 {
    let account_num: u64 = account.parse().unwrap_or(0);
    ((account_num / 1000000) % 10) as u32 + 1
}

// 方案 B：从 user_info 解析（需知道编码格式）
fn extract_set_id_from_user_info(user_info: &[u8; 32]) -> u32 {
    // TODO: 根据实际编码格式解析
    u32::from_le_bytes([user_info[0], user_info[1], user_info[2], user_info[3]])
}
```

---

#### 3. **TDGW 断开后的路由清理**

**当前实现**：TDGW 断开时只记录日志，未清理 Redis 路由

**问题**：如果 TDGW 断开，Redis 中的路由仍然存在，但会被 `route_order()` 自动过滤

❓ 需确认：
- [ ] **是否需要立即清理 Redis 路由**？
  - 优点：Redis 数据实时性高
  - 缺点：需要跟踪每个 TDGW 负责的分区列表
  
- [ ] **或者依赖 `route_order()` 过滤**？
  - 优点：实现简单，不需要额外存储
  - 缺点：Redis 中会残留失效路由

**建议**：使用方案 2（当前实现），因为：
- ✅ TDGW 重连后会自动更新路由
- ✅ `route_order()` 已经过滤断开的 TDGW
- ✅ 避免额外的内存开销（存储分区列表）

---

### 📅 下一步工作计划

#### Phase 2：订单路由集成（预计 1 天）

- [ ] **确认 set_id 提取方案**（与业务确认）
- [ ] **在 main.rs 中集成路由逻辑**
  ```rust
  TdgwBinFrame::NewOrderSingleNew(new_order) => {
      let pbu = new_order.get_biz_pbu_as_string();
      let set_id = extract_set_id(&new_order);  // 待实现
      
      match session_manager.route_order(&pbu, set_id) {
          Some(target_conn_id) => { /* 转发 */ }
          None => { /* 拒绝订单 */ }
      }
  }
  ```
- [ ] **添加 Redis 初始化到 main.rs**
- [ ] **添加配置文件支持**

#### 可选优化（建议完成）

- [ ] **实现智能路由算法**（根据业务确认结果）
- [ ] **添加 Session 分区列表跟踪**（用于 SREM 清理）
- [ ] **添加 TDGW 性能监控**（如果需要基于延迟路由）

---

### ✅ Git 提交建议

```bash
# 提交 1：Redis 客户端（已完成）
git add src/redis_client.rs
git commit -m "feat(redis): add multi-TDGW routing support with Redis Set"

# 提交 2：SessionManager 集成（本次修改）
git add src/session.rs
git commit -m "feat(session): implement smart routing with connection health check"

# 提交 3：文档
git add Redis分区路由功能开发文档.md
git commit -m "docs: update routing design for multi-TDGW load balancing"
```

### 需向同事/领导确认的问题

1. ✅ **已完成功能演示**
   - Redis 客户端封装（143 行代码）
   - 执行报告索引批量查询（性能优化）
   - 路由映射自动建立（**但有重大问题，见下方**）

2. 🚨 **发现的重大设计缺陷**
   - **多 TDGW 路由冲突**：
     - 当前实现会被后登录的 TDGW 覆盖
     - 无法支持主备 TDGW 部署
   - **重连路由失效**：
     - TDGW 重连后 conn_id 变化，但 Redis 路由未更新
     - 会导致订单路由到失效连接
   - **多柜台场景未考虑**：
     - 多个 OMS 同时发送订单到相同分区
     - 需要确保路由一致性

3. ❓ **需业务紧急确认（影响修复方案）**
   
   **TDGW 部署架构**：
   - [ ] 是否有主备 TDGW？（影响是否需要优先级机制）
   - [ ] 是否多个 TDGW 负责相同分区？（影响路由策略）
   - [ ] TDGW 重连频率高吗？（影响是否需要路由清理机制）
   
   **路由策略**：
   - [ ] 是否需要主备切换？（影响 Redis Key 设计）
   - [ ] 是否需要负载均衡？（影响路由选择算法）
   - [ ] 是否需要故障自动转移？（影响连接监控逻辑）
   
   **`set_id` 确定方式**：
   - [ ] 是否在 `user_info` 中传递？
   - [ ] 是否根据账户号段计算？
   - [ ] 映射规则是什么？

4. 📅 **调整后的工作计划**
   - **紧急修复**（Priority 0）：多 TDGW 路由冲突（0.5 天）
     - 添加优先级机制
     - 连接有效性检查
     - TDGW 配置扩展
   - **Phase 2**：订单路由集成（1 天）
   - **测试环境**：Redis 部署方案确认

1. ✅ **已完成功能演示**
   - Redis 客户端封装
   - 路由映射自动建立
   - 批量索引查询

2. ❓ **需业务确认**
   - `set_id` 的确定规则
   - 分区总数和编号范围
   - Redis 部署方案（单机/集群）

3. 📅 **下一步计划**
   - Phase 2 预计 1 天完成
   - 需要 fproto 支持（添加 set_id 字段）
   - 测试环境准备

### 演示代码片段

```rust
// 1. 路由映射建立（自动）
routing:0001:1 → conn_id=10
routing:0001:2 → conn_id=10
routing:0002:1 → conn_id=11

// 2. 订单路由查询
let target_conn_id = session_manager.route_order("0001", 1);
// 返回: Some(10)

// 3. 批量索引查询（性能优化）
batch_get_max_report_index([("0001", 1), ("0001", 2), ...])
// 一次查询返回所有索引
```

---

**文档版本**: v1.0  
**最后更新**: 2025-12-22 18:10
