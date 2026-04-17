# Redis功能缺口与待办任务

## 🚨 必须完成的Redis功能缺口

### **1. ID映射功能（核心缺失）**
**当前状态**: ❌ 完全未实现  
**需要实现**:
```rust
// HSET操作
- get_absolute_id_by_relative()  // a2r映射查询
- get_relative_id_by_absolute()  // r2a映射查询
- set_id_mapping()               // 设置双向映射
- batch_get_id_mapping()         // 批量ID转换
```
**用途**: UserInfo填充、回报解析gwid、数据恢复

---

### **2. GW配置管理（核心缺失）**
**当前状态**: ❌ 完全未实现  
**需要实现**:
```rust
// 字符串/JSON操作
- get_gw_list()           // share_offer_gw_list_
- get_gw_info()           // share_offer_gw_info_
- set_gw_list()
- set_gw_info()
```
**用途**: 共享报盘下的GW列表、平台配置信息

---

### **3. 最大reportIndex管理（Key前缀错误）**
**当前状态**: ⚠️ Key前缀不符合规范  
**需要修改**:
```rust
// 当前: exec_rpt_idx:{}:{}
// 应为: share_offer_max_reportIndex_{gwid}_{platformid}_{pbu}_{partitionNo}

- get_max_report_index()  // 修改Key格式
- set_max_report_index()  // 修改Key格式
- 增加10小时过期设置
```

---

### **4. 回报暂存（ZSET，核心缺失）**
**当前状态**: ❌ 完全未实现  
**需要实现**:
```rust
// ZSET操作
- zadd_flash_report()     // 添加回报(score=reportIndex)
- zrange_flash_report()   // 范围查询
- zrem_flash_report()     // 清理过期
- 设置10小时过期
```
**Key**: `share_offer_flash_report_{serverId}_{gwid}_{pbu}_{partitionNo}`

---

### **5. 现有功能需调整**

**分区路由routing** (当前已实现但需修改):
```rust
// 当前: routing:{}:{}  ❌
// 应为: share_offer_routing_{pbu}_{set_id} ✅
```

**回报存储exec_rpt** (当前已实现但需确认):
```rust
// 当前: exec_rpt:{}:{}:{}
// 文档未明确是否需要share_offer_前缀，需确认
// 10小时过期已实现但写死604800(7天)需改为36000(10小时)
```

---

## 📋 优先级排序

### **P0 - 立即修复(阻塞功能)**
1. ✅ 修改reportIndex Key前缀和TTL (当前已有逻辑)
2. ❌ 实现ID映射HSET操作 (UserInfo必需)
3. ❌ 实现回报暂存ZSET (重拉回报必需)

### **P1 - 近期完成(重要配置)**
4. ❌ 实现GW配置管理
5. ✅ 修改分区路由Key前缀
6. ✅ 统一回报存储TTL为10小时

### **P2 - 后续优化**
7. Redis Pipeline批量优化
8. 连接池管理
9. 监控告警

---

## 🛠️ 具体代码任务清单

```rust
// redis_client.rs 需新增方法：

// 1. ID映射 (HSET)
pub fn hget_id_mapping(&self, map_type: &str, key: &str) -> Result<Option<String>, RedisError>
pub fn hset_id_mapping(&self, map_type: &str, key: &str, value: &str) -> Result<(), RedisError>

// 2. GW配置
pub fn get_gw_list(&self, share_offer_id: &str) -> Result<Vec<String>, RedisError>
pub fn get_gw_info(&self, gw_id: &str) -> Result<Option<String>, RedisError>
pub fn set_gw_list(&self, share_offer_id: &str, gw_ids: &[String]) -> Result<(), RedisError>

// 3. 回报暂存 (ZSET)
pub fn zadd_flash_report(&self, server_id: &str, gw_id: &str, pbu: &str, 
                         partition_no: u32, report_index: u64, 
                         report_data: &[u8]) -> Result<(), RedisError>
pub fn zrange_flash_report(&self, ...) -> Result<Vec<(u64, Vec<u8>)>, RedisError>

// 4. 修改现有方法Key前缀
// - get_max_report_index/set_max_report_index
// - set_partition_routing/get_partition_routing
// - store_execution_report (确认是否需要前缀+改TTL为10小时)
```

---

## ⚡ 快速上手建议

**先做最小MVP验证**:
1. 修改`get_max_report_index`的Key前缀 (15分钟)
2. 实现`hget_id_mapping`/`hset_id_mapping` (30分钟)
3. 实现`zadd_flash_report`基础版 (30分钟)

然后再完善其他功能。

---

## 📊 当前代码现状对比

### 已实现功能
- ✅ Redis集群客户端封装
- ✅ 执行报告索引管理（Key前缀需调整）
- ✅ 分区路由缓存（Key前缀需调整）
- ✅ 批量查询优化
- ✅ 双层缓存机制（内存+Redis）
- ✅ OmsReportRouter回报路由
- ✅ 回报存储与重拉（TTL需调整）

### 缺失功能
- ❌ ID映射HSET操作
- ❌ GW配置管理
- ❌ 回报暂存ZSET
- ⚠️ Key前缀规范统一
- ⚠️ TTL时间调整（7天→10小时）
