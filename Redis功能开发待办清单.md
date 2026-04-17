# Redis功能开发待办清单

**开发者**: 郭帅 (guoshuai3)  
**更新时间**: 2026-01-12  
**项目**: share-offer 共享报盘

---

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

**Redis Key**:
- `share_offer_id_map_a2r_` - 相对ID→绝对ID
- `share_offer_id_map_r2a_` - 绝对ID→相对ID

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

**Redis Key**:
- `share_offer_gw_list_{共享报盘id}` - 下挂的gwid列表(逗号分隔)
- `share_offer_gw_info_{gwid}` - GW配置JSON

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

**当前代码位置**: `src/redis_client.rs` L20-45

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
**结构**: ZSET, score为reportIndex, value为reportInfo二进制

---

### **5. 现有功能需调整**

**分区路由routing** (当前已实现但需修改):
```rust
// 当前: routing:{}:{}  ❌
// 应为: share_offer_routing_{pbu}_{set_id} ✅
```
**代码位置**: `src/redis_client.rs` L74-103

**回报存储exec_rpt** (当前已实现但需确认):
```rust
// 当前: exec_rpt:{}:{}:{}
// 文档未明确是否需要share_offer_前缀，需确认
// 10小时过期已实现但写死604800(7天)需改为36000(10小时)
```
**代码位置**: `src/redis_client.rs` L112-201

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
8. 连接池