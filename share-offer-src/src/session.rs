use crate::config::oms::{self, OMSCONFIG};
use crate::config::tcp_share::TCPSHARECONFIG;
use crate::constants::{self, IdMapType};
use crate::oms_report_router::OmsReportRouter;
use crate::redis_client::{RedisClient, RedisWriteEvent, ExecReportEvent, GwStatusEvent, GwListEvent, GwInfoEvent};
use crate::{
    config::oms::OMSSession, config::tdgw::TDGWCONFIG, config::tdgw::TdgwSession,
    config::tgw::TGWCONFIG, config::tgw::TgwSession,
};
use fproto::stream_frame::{tdgw_bin, tgw_bin};
use share_offer_sys::tcp_connection;
use share_offer_sys::tcp_connection::{
    TCPConnection, tcp_conn_connect, tcp_conn_find_by_id, tcp_conn_listen, tcp_conn_strerror,
};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use chrono::Local;
use std::sync::Arc;
use std::vec;
use tracing::{debug, error, info, warn};

#[derive(Default, Debug, PartialEq)]
pub enum DetailConfig {
    #[default]
    None,
    OMSINFO(OMSSession),
    TDGWINFO(TdgwSession),
    TGWINFO(TgwSession),
}

impl DetailConfig {
    pub fn type_name(&self) -> &'static str {
        match self {
            DetailConfig::OMSINFO(_) => "OMS",
            DetailConfig::TDGWINFO(_) => "TDGW",
            DetailConfig::TGWINFO(_) => "TGW",
            DetailConfig::None => "None",
        }
    }
}

#[derive(Default)]
pub struct SessionManager {
    conn_id_2_session: HashMap<u16, Session>,
    session_to_reconnect: Vec<u16>,
    redis_client: Option<RedisClient>,
    oms_router: OmsReportRouter,
    partition_routing_cache: HashMap<(String, u32), u16>,
    //记录每个 TDGW conn_id 的 platform_id 集合，断线时用于向全部平台写 DISCONNECT
    seen_platform_ids: HashMap<u16, HashSet<u16>>,
}

#[derive(Default, Clone, Debug)]
pub struct Session {
    pub local_connect_str: String,
    pub remote_connect_str: String,
    pub remote_id: String,
    pub last_read_time_ms: u128,
    pub last_write_time_ms: u128,
    pub heart_beat: i32,
    pub time_out_count: u32,
    pub reconnect_interval: u16,
    pub status: SessionStatus,
    pub conn_id: u16,
    pub route_id: u16,
    pub conn_tag: String,
    pub conn_type: ConnType,
    pub session_type: SessionType,
    pub conn: TCPConnection,
    pub detail_config: Arc<DetailConfig>,
}

#[derive(Default, Clone, PartialEq, Debug)]
pub enum SessionStatus {
    #[default]
    Disconnected, // initial or get tcp closed event
    Connected,      // get tcp connected event
    LoggedIn,       // reply oms / get tdgw logon response
    Ready,          // syn platform state && reportIndex
    WaitDisconnect, // need to disconnect
    Closing,        // run tcp_conn_close success, or closed by gw/oms
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Disconnected  => "Disconnected",
            SessionStatus::Connected     => "Connected",
            SessionStatus::LoggedIn      => "LoggedIn",
            SessionStatus::Ready         => "Ready",
            SessionStatus::WaitDisconnect => "WaitDisconnect",
            SessionStatus::Closing       => "Closing",
        }
    }
}

#[derive(Default, Clone, PartialEq, Debug)]
pub enum SessionType {
    #[default]
    OMS,
    TGW,
    TDGW,
}

#[derive(Default, Clone, PartialEq, Debug)]
pub enum ConnType {
    #[default]
    CLIENT,
    SERVER,
}

impl SessionManager {
    pub fn init_redis_from_config(&mut self, nodes: Vec<String>) -> Result<(), String> {
        info!(target: "system", "init_redis_from_config: starting with {} nodes", nodes.len());
        if nodes.is_empty() {
            error!(target: "system", "Redis nodes not configured, system cannot start");
            return Err("Redis nodes cannot be empty".to_string());
        }
        info!(target: "system", "init_redis_from_config: connecting to Redis cluster (timeout=10s)...");

        // 在子线程中执行连接+ping，主线程最多等 10 秒
        let (tx, rx) = crossbeam_channel::bounded::<Result<RedisClient, String>>(1);
        let nodes_clone = nodes.clone();
        std::thread::spawn(move || {
            let result = RedisClient::new_cluster(nodes_clone)
                .map_err(|e| format!("create client failed: {:?}", e))
                .and_then(|client| {
                    client.ping().map_err(|e| format!("ping failed: {:?}", e))?;
                    Ok(client)
                });
            let _ = tx.send(result);
        });

        match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Ok(client)) => {
                info!(target: "system", "init_redis_from_config: ping success, Redis ready");
                self.redis_client = Some(client);
                info!(target: "system", "Redis cluster initialized: {} nodes", nodes.len());
                Ok(())
            }
            Ok(Err(e)) => {
                error!(target: "system", "Redis cluster connection failed: {}", e);
                Err(e)
            }
            Err(_) => {
                error!(target: "system", "Redis cluster connection timed out after 10 seconds");
                error!(target: "system", "Ports reachable but Redis protocol not responding. Check cluster health or auth.");
                Err("Redis connection timed out after 10s".to_string())
            }
        }
    }


    pub fn init_id_mapping(&mut self) -> Result<(), String> {
        if let Some(ref redis_client) = self.redis_client {
            let share_id = TCPSHARECONFIG.share_offer_id.to_string();

            let mut relative_counter = 0u16;

            let mut sorted_sessions: Vec<(&u16, &Session)> = self.conn_id_2_session.iter().collect();
            sorted_sessions.sort_by_key(|(conn_id, _)| *conn_id);

            for (conn_id, session) in sorted_sessions {
                if relative_counter >= 16 {
                    error!(target: "system", "Too many sessions: maximum 16 connections (OMS + TdGW) allowed");
                    return Err("Maximum 16 sessions exceeded".to_string());
                }

                let (absolute_id, platform_id_opt) = match &*session.detail_config {
                    DetailConfig::OMSINFO(oms_config) => {
                        (oms_config.server_id.clone(), Some(oms_config.platform_id))
                    },
                    DetailConfig::TDGWINFO(_) => {
                        (format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + session.route_id as u32), None)
                    },
                    DetailConfig::TGWINFO(_) => {
                        (format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + session.route_id as u32), None)
                    },
                    DetailConfig::None => continue,
                };

                let id_type = session.detail_config.type_name();

                if absolute_id.is_empty() {
                    warn!(target: "system", "Skip empty absolute_id: type={}, conn_id={}", id_type, conn_id);
                    continue;
                }

                let relative_id = (65 + relative_counter) as u8 as char;
                let relative_id_str = relative_id.to_string();
                relative_counter += 1;

                let composite_id = if let Some(pid) = platform_id_opt {
                    format!("{}_{}", relative_id_str, pid)
                } else {
                    relative_id_str.clone()
                };

                #[cfg(debug_assertions)]
                debug!(target: "system", "Mapping session: conn_id={}, type={}, absolute_id={}, relative_id={} ({}), composite_id={}",
                       conn_id, id_type, absolute_id, relative_id, relative_id as u8, composite_id);

                // absolute_id → composite_id（serverId→relative_id_platformId 或 gwid→relative_id）
                if let Err(e) = redis_client.hset_id_mapping(
                    IdMapType::A2R,
                    &share_id,
                    &absolute_id,
                    &composite_id
                ) {
                    error!(target: "system", "Failed to set a2r mapping for {}: {:?}", id_type, e);
                    return Err(format!("Failed to set a2r mapping: {:?}", e));
                }

                // Set r2a mapping: composite_id → absolute_id（relative_id_platformId→serverId 或 relative_id→gwid）
                if let Err(e) = redis_client.hset_id_mapping(
                    IdMapType::R2A,
                    &share_id,
                    &composite_id,
                    &absolute_id
                ) {
                    error!(target: "system", "Failed to set r2a mapping for {}: {:?}", id_type, e);
                    return Err(format!("Failed to set r2a mapping: {:?}", e));
                }

                info!(target: "system", "ID mapping initialized: type={}, conn_id={}, relative='{}' ({}), composite_id={}, absolute={}",
                      id_type, conn_id, relative_id, relative_id as u8, composite_id, absolute_id);
            }

            info!(target: "system", "All ID mappings initialized: {} total sessions (max 16)", relative_counter);
            Ok(())
        } else {
            error!(target: "system", "Redis not initialized, cannot set ID mappings");
            Err("Redis not initialized".to_string())
        }
    }

    pub fn record_partition_routing_from_report(
        &mut self,
        pbu: &str,
        set_id: u32,
        tdgw_conn_id: u16,
    ) {
        let key = (pbu.to_string(), set_id);

        if let Some(&cached_gw_id) = self.partition_routing_cache.get(&key) {
            if cached_gw_id != tdgw_conn_id {
                warn!(target: "business", 
                      "partition routing changed: pbu={}, set_id={}, old_gw={}, new_gw={}",
                      pbu, set_id, cached_gw_id, tdgw_conn_id);
            }
        }

        self.partition_routing_cache.insert(key, tdgw_conn_id);
        debug!(target: "business", 
               "partition routing learned: pbu={}, set_id={}, gw_conn_id={}",
               pbu, set_id, tdgw_conn_id);
    }

    pub fn update_partition_routing(&mut self, pbu: &str, set_id: u32, gw_conn_id: u16) {
        let key = (pbu.to_string(), set_id);
        self.partition_routing_cache.insert(key, gw_conn_id);
        debug!(target: "business", "partition routing manually updated: pbu={}, set_id={}, gw_conn_id={}",
               pbu, set_id, gw_conn_id);

        if let Some(ref redis_client) = self.redis_client {
            let route_id = self
                .conn_id_2_session
                .get(&gw_conn_id)
                .map(|session| session.route_id);
            match route_id {
                Some(route_id) => {
                    if let Err(e) = redis_client.set_partition_routing(
                        TCPSHARECONFIG.share_offer_id,
                        pbu,
                        set_id,
                        route_id,
                    ) {
                        error!(target: "system", "failed to persist to Redis: {:?}", e);
                    }
                }
                None => {
                    error!(target: "system", "failed to persist routing: missing session for conn_id={}", gw_conn_id);
                }
            }
        }
    }

    pub fn remove_partition_routing(&mut self, pbu: &str, set_id: u32) {
        let key = (pbu.to_string(), set_id);
        self.partition_routing_cache.remove(&key);
        debug!(target: "business", "partition routing removed: pbu={}, set_id={}", pbu, set_id);

        if let Some(ref redis_client) = self.redis_client {
            if let Err(e) = redis_client.remove_partition_routing(TCPSHARECONFIG.share_offer_id, pbu, set_id) {
                error!(target: "system", "failed to remove from Redis: {:?}", e);
            }
        }
    }

    pub fn find_tdgw_by_gw_id(&self, gw_id: u16) -> Option<u16> {
        self.conn_id_2_session
            .iter()
            .find(|(_, session)| {
                session.session_type == SessionType::TDGW
                    && session.status == SessionStatus::Ready
                    && (session.remote_id.contains(&gw_id.to_string())
                        || session.conn_tag.contains(&gw_id.to_string()))
            })
            .map(|(conn_id, _)| *conn_id)
    }

    pub fn route_order_cached(&self, pbu: &str, set_id: u32) -> Option<u16> {
        let key = (pbu.to_string(), set_id);
        match self.partition_routing_cache.get(&key) {
            Some(&conn_id) => {
                if self
                    .conn_id_2_session
                    .get(&conn_id)
                    .map(|s| s.status == SessionStatus::Ready)
                    .unwrap_or(false)
                {
                    debug!(target: "business", "order routed : pbu={}, set_id={}, conn_id={}",
                           pbu, set_id, conn_id);
                    Some(conn_id)
                } else {
                    warn!(target: "business", "cached route invalid: conn_id={} not ready", conn_id);
                    None
                }
            }
            None => {
                warn!(target: "business", "no route found: pbu={}, set_id={}", pbu, set_id);
                None
            }
        }
    }

    pub fn record_order(&mut self, contract_num: [u8;10], oms_conn_id: u16) {
        self.oms_router.record_order(contract_num, oms_conn_id);
        debug!(target: "business", "order recorded: contract_num={:?}, oms_id={}", 
               contract_num, oms_conn_id);
    }

    pub fn route_report(&mut self, contract_num: &[u8;10]) -> Option<u16> {
        match self.oms_router.route_report(contract_num) {
            Some(oms_conn_id) => {
                debug!(target: "business", "report routed: contract_num={:?}, oms_id={}", 
                       contract_num, oms_conn_id);
                Some(oms_conn_id)
            }
            None => {
                warn!(target: "business", "report routing failed: contract_num={:?} not found", 
                      contract_num);
                self.oms_router.print_all_record();
                None
            }
        }
    }

    /// TODO
    pub fn get_conn_id_by_oms_id(&self, target_oms_id: u16) -> Option<u16> {
        self.conn_id_2_session
            .iter()
            .find(|(_, session)| {
                if session.session_type != SessionType::OMS {
                    return false;
                }
                if session.status != SessionStatus::Ready
                    && session.status != SessionStatus::LoggedIn
                {
                    return false;
                }
                session.remote_id.contains(&target_oms_id.to_string())
                    || session.conn_tag.contains(&target_oms_id.to_string())
            })
            .map(|(conn_id, _)| *conn_id)
    }

    pub fn on_oms_disconnect(&mut self, conn_id: u16) {
        info!(target: "system", "OMS disconnected: conn_id={}", conn_id);
    }

    pub fn gw_begin_reconnect(&mut self, conn_id: u16) {
        if !self.session_to_reconnect.contains(&conn_id) {
            self.session_to_reconnect.push(conn_id);
        } else {
            info!(target:"system","session manager has already to reconnect gw conn_id:{:?}",conn_id)
        }
    }

    pub fn store_execution_report(
        &mut self,
        conn_id: u16,
        pbu: &str,
        set_id: u32,
        report_index: u64,
        report_data: &[u8],
    ) -> Result<(), String> {
        if let Some(ref redis_client) = self.redis_client {
            let (server_id, route_id) = if let Some(session) = self.conn_id_2_session.get(&conn_id) {
                match &*session.detail_config {
                    DetailConfig::OMSINFO(oms_config) => {
                        (oms_config.server_id.clone(), 0u16)
                    },
                    DetailConfig::TDGWINFO(_) | DetailConfig::TGWINFO(_) => {
                        (
                            format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + session.route_id as u32),
                            session.route_id,
                        )
                    },
                    DetailConfig::None => return Err(format!("Session {} has no detail_config", conn_id)),
                }
            } else {
                return Err(format!("Session not found for conn_id={}", conn_id));
            };

            redis_client
                .store_execution_report(
                    TCPSHARECONFIG.share_offer_id,
                    &server_id,
                    route_id,
                    pbu,
                    set_id,
                    report_index,
                    report_data,
                )
                .map_err(|e| format!("Failed to store report to Redis: {:?}", e))?;

            redis_client
                .set_max_report_index(
                    TCPSHARECONFIG.share_offer_id,
                    route_id,
                    pbu,
                    set_id,
                    report_index,
                )
                .map_err(|e| format!("Failed to update report index: {:?}", e))?;

            Ok(())
        } else {
            warn!(target: "business", "Redis not initialized, report not stored");
            Err("Redis not initialized".to_string())
        }
    }

    pub fn build_store_event(
        &self,
        conn_id: u16,
        pbu: &str,
        set_id: u32,
        report_index: u64,
        report_data: Vec<u8>,
    ) -> Option<RedisWriteEvent> {
        let session = self.conn_id_2_session.get(&conn_id)?;
        let server_id = match &*session.detail_config {
            DetailConfig::OMSINFO(oms_config) => {
                oms_config.server_id.clone()
            }
            DetailConfig::TDGWINFO(_) => {
                format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + session.route_id as u32)
            }
            DetailConfig::TGWINFO(_) => {
                format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + session.route_id as u32)
            }
            DetailConfig::None => return None,
        };
        Some(RedisWriteEvent::ExecReport(ExecReportEvent {
            server_id,
            share_offer_id: TCPSHARECONFIG.share_offer_id,
            route_id: session.route_id,
            pbu: pbu.to_string(),
            partition_no: set_id,
            report_index,
            report_data,
        }))
    }

    pub fn get_latest_report_index(&self, pbu: &str, set_id: u32) -> Result<u64, String> {
        if let Some(ref rc) = self.redis_client {
            let route_id = self.partition_routing_cache
                .get(&(pbu.to_string(), set_id))
                .and_then(|&conn_id| self.conn_id_2_session.get(&conn_id))
                .map(|s| s.route_id)
                .unwrap_or_default();
            rc.get_max_report_index(TCPSHARECONFIG.share_offer_id, route_id, pbu, set_id)
                .map_err(|e| format!("Redis get_max_report_index error: {:?}", e))
        } else {
            Err("Redis not initialized".to_string())
        }
    }

    pub fn batch_get_execution_reports(
        &self,
        pbu: &str,
        set_id: u32,
        begin_index: u64,
        latest_index: u64,
    ) -> Result<Vec<(u64, Vec<u8>)>, String> {
        if let Some(ref rc) = self.redis_client {
            let route_info = self.partition_routing_cache
                .get(&(pbu.to_string(), set_id))
                .and_then(|&conn_id| self.conn_id_2_session.get(&conn_id))
                .map(|s| {
                    (
                        s.route_id,
                        format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + s.route_id as u32),
                    )
                });
            let (route_id, server_id) = route_info.unwrap_or_else(|| (0u16, String::new()));
            rc.batch_get_execution_reports(
                TCPSHARECONFIG.share_offer_id,
                &server_id,
                route_id,
                pbu,
                set_id,
                begin_index,
                latest_index,
            )
                .map_err(|e| format!("Redis batch_get_execution_reports error: {:?}", e))
        } else {
            Err("Redis not initialized".to_string())
        }
    }

    pub fn add_session(&mut self, session: Session) {
        self.conn_id_2_session.insert(session.conn_id, session);
    }

    #[inline(always)]
    pub fn get_session_by_conn_id(&self, conn_id: u16) -> Option<&Session> {
        self.conn_id_2_session.get(&conn_id)
    }

    pub fn remove_session_by_conn_id(&mut self, conn_id: u16) -> Option<Session> {
        let removed_session = self.conn_id_2_session.remove(&conn_id);
        removed_session
    }

    pub fn set_session_status_by_conn_id(&mut self, conn_id: u16, status: SessionStatus) {
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => {
                warn!(target: "business", "logout failed: session not found, conn_id={}, target_status={:?}",
                      conn_id, status);
            }
            Some(session) => {
                let old_status = session.status.clone();
                session.status = status;
                info!(target: "business", "session status change: conn_id={}, {:?} -> {:?}, session_type={:?}",
                      conn_id, old_status, session.status, session.session_type);
            }
        }
    }

    pub fn add_session_to_reconnect(&mut self, conn_id: u16) {
        if !self.session_to_reconnect.contains(&conn_id) {
            self.session_to_reconnect.push(conn_id);
            info!(target: "system", "add_session_to_reconnect: conn_id={}",conn_id);
        }
    }

    pub fn on_platform_status(&mut self, conn_id: u16, platform_status: u16) -> bool {
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => false,
            Some(session) => {
                if session.status == SessionStatus::LoggedIn {
                    session.status = SessionStatus::Ready;
                    info!(target: "system", "on_platform_status: conn_id={:?},cur_session_statu={:?},gw_platform_status={:?}",session.conn_id,session.status,platform_status);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn update_read_time_by_conn_id(&mut self, now: u128, conn_id: u16) {
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => {}
            Some(session) => {
                session.last_read_time_ms = now;
                session.time_out_count = 0;
            }
        }
    }

    pub fn update_write_time_by_conn_id(&mut self, now: u128, conn_id: u16) {
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => {}
            Some(session) => {
                session.last_write_time_ms = now;
                //session.time_out_count = 0;
            }
        }
    }

    pub fn get_ready_gw_conn_ids(&self) -> HashMap<u16, Vec<u16>> {
        let mut gw_ready_map: HashMap<u16, Vec<u16>> = HashMap::new();
        //待确认，后面考虑增加conn的状态检查
        self.conn_id_2_session
            .iter()
            .for_each(|(conn_id, session)| {
                if session.status == SessionStatus::Ready {
                    // 根据 feature 配置筛选 TGW 或 TDGW
                    #[cfg(feature = "tgw")]
                    {
                        if session.session_type == SessionType::TGW {
                            let _ = match **&session.detail_config {
                                DetailConfig::TGWINFO(ref tgw_detail) => {
                                    gw_ready_map
                                        .entry(tgw_detail.platform_id)
                                        .or_default()
                                        .push(*conn_id);
                                }
                                _ => {}
                            };
                        }
                    }
                    #[cfg(feature = "tdgw")]
                    {
                        if session.session_type == SessionType::TDGW {
                            let _ = match **&session.detail_config {
                                DetailConfig::TDGWINFO(ref tdgw_detail) => {
                                    gw_ready_map
                                        .entry(tdgw_detail.platform_id)
                                        .or_default()
                                        .push(*conn_id);
                                }
                                _ => {}
                            };
                        }
                    }
                }
            });
        gw_ready_map
    }

    ///待确认，共享报盘的报盘状态，用于向oms回复platform_status
    ///  0 = NotOpen，未开放 => 回复Order Reject（OrdRejReason=5009）
    ///  2 = Open，开放      => 处理Order
    pub fn get_so_platform_status(&self) -> u16 {
        if self.get_ready_gw_conn_ids().len() > 0 {
            constants::TDGW_PLATFORM_STATE_OPEN_2
        } else {
            constants::TDGW_PLATFORM_STATE_NOTOPEN_0
        }
    }

    pub fn send_so_platform_status_to_oms(
        &mut self,
        conn_id: u16,
        now: u128,
        so_status: u16,
    ) -> Result<(), String> {
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => Err(format!(
                "send_so_platform_status_to_oms, can't found conn id:{:?}",
                conn_id
            )),
            Some(session) => {
                let gw_info = match **&session.detail_config {
                    DetailConfig::OMSINFO(ref gw_detail) => Ok(gw_detail),
                    _ => Err(format!(
                        "unreachalbe!,Connection detail not found!,connd_id:{:?},config:{:?}",
                        conn_id, &session.detail_config
                    )),
                }?;
                #[cfg(feature = "tdgw")]
                {
                    let mut platform_state = tdgw_bin::platform_state::PlatformState::new();
                    platform_state.set_platform_id(gw_info.platform_id);
                    platform_state.set_platform_state(so_status);
                    platform_state.filled_head_and_tail();
                    match session
                        .conn
                        .tcp_conn_send_bytes(&platform_state.as_bytes_big_endian())
                    {
                        Err(e) => {
                            warn!(target: "business","send tdgw platform_state to oms error:conn_id={},{}",conn_id,e);
                            session.status = SessionStatus::WaitDisconnect;
                            Err(e.to_string())
                        }
                        Ok(_) => {
                            info!(target:"messages::share_offer::out::tdgw","{:?}, {:?}, {}",conn_id,now,platform_state);
                            Ok(())
                        }
                    }
                }
                #[cfg(feature = "tgw")]
                {
                    let mut platform_state = tgw_bin::platform_state_info::PlatformStateInfo::new();
                    platform_state.set_platform_id(gw_info.platform_id);
                    platform_state.set_platform_state(so_status);
                    platform_state.filled_head_and_tail();
                    match session
                        .conn
                        .tcp_conn_send_bytes(&platform_state.as_bytes_big_endian())
                    {
                        Err(e) => {
                            warn!(target: "business","send tgw platform_state to oms error:conn_id={},{}",conn_id,e);
                            session.status = SessionStatus::WaitDisconnect;
                            Err(e.to_string())
                        }
                        Ok(_) => {
                            info!(target:"messages::share_offer::out::tgw","conn_id={:?},time={:?},msg={:?}",conn_id,now,platform_state);
                            Ok(())
                        }
                    }
                }
            }
        }
    }

    pub fn get_logged_in_oms_conn_ids(&self) -> Vec<u16> {
        self.conn_id_2_session
            .iter()
            .filter(|(_, session)| {
                session.session_type == SessionType::OMS
                    && (session.status == SessionStatus::LoggedIn
                        || session.status == SessionStatus::Ready)
            })
            .map(|(conn_id, _)| *conn_id)
            .collect()
    }

    pub fn process_gw_platform_state_msg(
        &mut self,
        now: u128,
        gw_conn_id: u16,
        platform_id: u16,
        platform_state: u16,
    ) -> (bool, bool) {
        let has_gw_ready = self.on_platform_status(gw_conn_id, platform_state);
        let session_tag = self
            .conn_id_2_session
            .get(&gw_conn_id)
            .map(|s| s.conn_tag.clone())
            .unwrap_or_else(|| "unknown".to_string());

        info!(target: "business", 
              "gw platform state: conn_id={}, session={}, platform_id={}, platform_state={}",
              gw_conn_id, session_tag, platform_id, platform_state);
        let oms_conn_ids = self.get_logged_in_oms_conn_ids();
        if !oms_conn_ids.is_empty() {
            info!(target: "business", 
                  "Pushing platform_id={}, platform_state={} to {} OMS connections",
                  platform_id, platform_state, oms_conn_ids.len());

            for oms_conn_id in oms_conn_ids {
                if let Err(err) =
                    self.send_so_platform_status_to_oms(oms_conn_id, now, platform_state)
                {
                    error!(
                        "tgw 2 oms send platform error,conn:{:?},platformid:{:?},plateState:{:?},error:{:?}",
                        oms_conn_id, platform_id, platform_state, err
                    )
                }
            }
            return (true, has_gw_ready);
        }
        (false, has_gw_ready)
    }

    pub fn process_session_connected_event(&mut self, now: u128, conn_id: u16) {
        let session = match self.conn_id_2_session.get_mut(&conn_id) {
            None => {
                warn!(target: "system", "failed get session in connected handle, should not happen, conn_id={:?}", conn_id);
                return;
            }
            Some(session) => session,
        };

        let info = session.conn.tcp_get_conn_info();
        session.status = SessionStatus::Connected;
        session.time_out_count = 0;
        session.last_read_time_ms = now;
        session.last_write_time_ms = now;

        match session.session_type {
            SessionType::OMS => {
                info!(
                    target: "system",
                    "oms: {}:{} -> share_offer: {}:{} has connected.",
                    info.get_remote_ip(),
                    info.get_conn_id(),
                    info.get_local_ip(),
                    info.get_local_port()
                );
            }
            SessionType::TDGW => {
                #[cfg(feature = "tdgw")]
                {
                    info!(
                        target: "system",
                        "share_offer: {}:{} -> tdgw: {}:{} has connected.",
                        info.get_local_ip(),
                        info.get_local_port(),
                        info.get_remote_ip(),
                        info.get_remote_port()
                    );

                    let mut logon = tdgw_bin::logon::Logon::new();
                    let tdgwsession = TDGWCONFIG
                        .session_id_to_session_map
                        .get(&session.conn_tag)
                        .expect(&format!(
                            "get tdgw config fail, conn_tag={}",
                            session.conn_tag
                        ));

                    logon.set_sender_comp_id_from_string(&tdgwsession.sender_comp_id);
                    logon.set_target_comp_id_from_string(&tdgwsession.target_comp_id);
                    logon.set_prtcl_version_from_string(&TDGWCONFIG.prtcl_version);
                    logon.set_heart_bt_int(TDGWCONFIG.heart_bt_int as u16);
                    logon.filled_head_and_tail();

                    match session
                        .conn
                        .tcp_conn_send_bytes(&logon.as_bytes_big_endian())
                    {
                        Ok(_) => {
                            info!(target: "messages::share_offer::out::tdgw","send tdgw logon success,conn_id={}",session.conn_id);
                            session.last_write_time_ms = now;
                        }
                        Err(e) => {
                            warn!(target: "system","share_offer: send tdgw logon fail: {:?}", e);
                            session.status = SessionStatus::WaitDisconnect;
                        }
                    }
                }
            }
            SessionType::TGW => {
                #[cfg(feature = "tgw")]
                {
                    info!(
                        target: "system",
                        "share_offer: {}:{} -> tgw: {}:{} has connected.",
                        info.get_local_ip(),
                        info.get_local_port(),
                        info.get_remote_ip(),
                        info.get_remote_port()
                    );
                    let mut logon = tgw_bin::logon::Logon::new();
                    let tgwsession = TGWCONFIG
                        .session_id_to_session_map
                        .get(&session.conn_tag)
                        .expect(&format!(
                            "get tgw config fail, conn_tag={}",
                            session.conn_tag
                        ));

                    logon.set_sender_comp_id_from_string(&tgwsession.sender_comp_id);
                    logon.set_target_comp_id_from_string(&tgwsession.target_comp_id);
                    logon.set_default_appl_ver_id_from_string(&TGWCONFIG.default_appl_ver_id);
                    logon.set_heart_bt_int(TGWCONFIG.heart_bt_int);
                    logon.set_password_from_string(&tgwsession.password);
                    logon.filled_head_and_tail();

                    match session
                        .conn
                        .tcp_conn_send_bytes(&logon.as_bytes_big_endian())
                    {
                        Ok(_) => {
                            info!(target: "messages::share_offer::out::tgw","conn_id={:?},time={:?},msg={:?}",session.conn_id,now,logon);
                            session.last_write_time_ms = now;
                        }
                        Err(e) => {
                            warn!(target: "system","share_offer: send tgw logon fail: {:?}", e);
                            session.status = SessionStatus::WaitDisconnect;
                        }
                    }
                }
            }
        }
    }

    /// 1. if now - lastRead > heartBeat, count + 1, if count > 2, disconnect
    /// 2. if now - lastWrite > heartBeat, send heartBeat, update lastWrite
    pub fn process_heart_beats_event(&mut self, now: u128) {
        for (conn_id, session) in self.conn_id_2_session.iter_mut() {
            match session.status {
                SessionStatus::LoggedIn | SessionStatus::Ready => {
                    // 检测心跳超时
                    if now - session.last_read_time_ms
                        > session.heart_beat as u128 * 1000 * 1000 * 1000
                    {
                        if session.time_out_count == 2 {
                            info!(
                                target: "system","heart process:close conn due to heartbeat timeout,now={:?},last_read_time_ms={:?}, hearbeat={:?},conn_id={:?}",
                                now,
                                session.last_read_time_ms,
                                session.heart_beat as u128 * 1000 * 1000 * 1000,
                                conn_id
                            );
                            //超时关闭
                            session.status = SessionStatus::WaitDisconnect;
                            continue;
                        } else {
                            session.time_out_count += 1;
                            debug!(
                                "heart process:heartbeat timeout,now={:?},time_out_count={:?},last_read_time_ms={:?}, hearbeat={:?},conn_id={:?}",
                                now,
                                session.time_out_count,
                                session.last_read_time_ms,
                                session.heart_beat as u128 * 1000 * 1000 * 1000,
                                conn_id
                            );
                            session.last_read_time_ms = now;
                        }
                    }
                    if now - session.last_write_time_ms
                        > session.heart_beat as u128 * 1000 * 1000 * 1000
                    {
                        //发往柜台的心跳proto，和当前共享报盘连接的交易网关类型一致
                        //send gw/oms hb
                        #[cfg(feature = "tdgw")]
                        {
                            let mut heartbeat = tdgw_bin::heartbeat::Heartbeat::new();
                            heartbeat.filled_head_and_tail();
                            match session
                                .conn
                                .tcp_conn_send_bytes(&heartbeat.as_bytes_big_endian())
                            {
                                Ok(_) => {
                                    info!(target: "messages::share_offer::out", "heartbeat,conn_id={:?},msg={:?}",conn_id,heartbeat);
                                    session.last_write_time_ms = now;
                                }
                                Err(error) => {
                                    error!(
                                        "send tdgw heartbeat fail,conn_id={:?} ,{:?},",
                                        conn_id, error
                                    );
                                }
                            }
                        }
                        #[cfg(feature = "tgw")]
                        {
                            let mut heartbeat = tgw_bin::heartbeat::Heartbeat::new();
                            heartbeat.filled_head_and_tail();
                            match session
                                .conn
                                .tcp_conn_send_bytes(&heartbeat.as_bytes_big_endian())
                            {
                                Ok(_) => {
                                    info!(target: "messages::share_offer::out::tgw", "heartbeat,conn_id={:?},msg={:?}",conn_id,heartbeat);
                                    session.last_write_time_ms = now;
                                }
                                Err(error) => {
                                    error!(
                                        "send tgw heartbeat fail,conn_id={:?} ,{:?},",
                                        conn_id, error
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // 登录后收到执行报告分区信息 ->获取分区信息 ->从redis获取maxreportIndex-> 发送分区序号同步
    pub fn process_tdgw_exec_rpt_info_msg(
        &mut self,
        now: u128,
        conn_id: u16,
        execrptinfo: &tdgw_bin::exec_rpt_info::ExecRptInfo,
    ) {
        let mut pbu_set_pairs = vec![];
        for pbu in execrptinfo.get_no_groups_4() {
            for set_id in execrptinfo.get_no_groups_5() {
                pbu_set_pairs.push((
                    String::from_utf8_lossy(pbu.get_pbu()).trim().to_string(),
                    set_id.get_set_id(),
                ));
            }
        }
        let report_indexes = if let Some(ref redis_client) = self.redis_client {
            let route_id = if let Some(session) = self.conn_id_2_session.get(&conn_id) {
                match &*session.detail_config {
                    DetailConfig::TDGWINFO(_) | DetailConfig::TGWINFO(_) => session.route_id,
                    _ => 0u16,
                }
            } else {
                error!(target: "business", "Session not found for conn_id={}", conn_id);
                0u16
            };

            match redis_client.batch_get_max_report_index(
                TCPSHARECONFIG.share_offer_id,
                route_id,
                &pbu_set_pairs,
            ) {
                Ok(indexes) => indexes,
                Err(e) => {
                    error!(target: "business", "process tdgw execrptinfo: Redis query failed: {:?}, using default", e);
                    pbu_set_pairs
                        .iter()
                        .map(|(pbu, set_id)| ((pbu.clone(), *set_id), 1u64))
                        .collect()
                }
            }
        } else {
            pbu_set_pairs
                .iter()
                .map(|(pbu, set_id)| ((pbu.clone(), *set_id), 1u64))
                .collect()
        };

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

        //分区序号同步
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => {
                error!(
                    "process tdgw execrptinfo:should not happen,conn_id={:?}",
                    conn_id
                );
            }
            Some(session) => {
                let mut exec_rpt_sync = tdgw_bin::exec_rpt_sync::ExecRptSync::new();
                exec_rpt_sync.set_no_groups_3(&v);
                exec_rpt_sync.filled_head_and_tail();

                match session.conn.tcp_conn_send_bytes(&exec_rpt_sync.as_bytes()) {
                    Ok(_) => {
                        info!(target: "messages::tdgw::out", "{:?}, {:?}, {}",conn_id,now,exec_rpt_sync);
                        session.last_write_time_ms = now;
                    }
                    Err(error) => {
                        error!(
                            "process tdgw execrptinfo: send tdgw exec_rpt_sync fail: {:?},conn_id={:?}",
                            error, conn_id
                        );
                        session.status = SessionStatus::WaitDisconnect;
                    }
                }
            }
        }
    }

    //分区序号同步响应处理
    pub fn process_tdgw_exec_rpt_sync_rsp_msg(
        &mut self,
        conn_id: u16,
        exec_rpt_sync_rsp: &tdgw_bin::exec_rpt_sync_rsp::ExecRptSyncRsp,
    ) {
        let session = self.conn_id_2_session.get_mut(&conn_id).unwrap();
        let cur_statu = session.status.clone();
        match cur_statu {
            SessionStatus::LoggedIn | SessionStatus::Ready => {
                //todo,处理分区序号同步响应,redis记录repordindex等
                //cur_statu的case条件只是暂定，根据实际情况调整
            }
            _ => {
                warn!(
                    "get tdgw rpt syn in wrong status: conn_id={:?},status={:?}",
                    conn_id, session.status
                );
            }
        }
    }

    //收到oms_logon -> 回复logon,status=LoggedIn -> 回复平台状态信息 ->回复execrptinfo
    pub fn process_oms_logon_msg_tdgw(
        &mut self,
        now: u128,
        conn_id: u16,
        oms_logon: &tdgw_bin::logon::Logon,
    ) -> bool {
        let mut result = false;
        let session = self.conn_id_2_session.get_mut(&conn_id).unwrap();
        let cur_statu = session.status.clone();
        let conn_info = session.conn.tcp_get_conn_info();
        let remote_ip = conn_info.get_remote_ip();
        if !OMSCONFIG.is_ip_allowed(&remote_ip) {
            warn!(target: "business", "IP not in whitelist: conn_id={:?}, remote_ip={}, rejecting login", 
                  conn_id, remote_ip);
            let mut logout = tdgw_bin::logout::Logout::new();
            logout.set_text_from_string("IP address not in whitelist");
            logout.filled_head_and_tail();
            match session
                .conn
                .tcp_conn_send_bytes(&logout.as_bytes_big_endian())
            {
                Ok(_) => {
                    info!(target: "business", "IP whitelist rejection: logout sent, conn_id={:?}, remote_ip={}", 
                          conn_id, remote_ip);
                }
                Err(error) => {
                    error!(target: "business", "IP whitelist rejection: send logout fail, conn_id={:?}, error={:?}", 
                           conn_id, error);
                }
            }
            session.status = SessionStatus::WaitDisconnect;
            return false;
        }
        info!(target: "business", "IP whitelist check passed: conn_id={:?}, remote_ip={}", conn_id, remote_ip);
        match cur_statu {
            SessionStatus::Connected => {
                //reply logon
                match session
                    .conn
                    .tcp_conn_send_bytes(&oms_logon.as_bytes_big_endian())
                {
                    Err(error) => {
                        error!(
                            "send fail: conn_id={:?},error={:?},msg={:?}",
                            conn_id, error, oms_logon
                        );
                        session.status = SessionStatus::WaitDisconnect;
                    }
                    Ok(_) => {
                        session.status = SessionStatus::LoggedIn;
                        session.last_write_time_ms = now;
                        result = true;
                        if oms_logon.get_heart_bt_int() > 0 {
                            session.heart_beat = oms_logon.get_heart_bt_int() as i32;
                        } else {
                            warn!(target: "business", "oms logon, hearbeat value invalid: conn_id={:?},msg={}", conn_id, oms_logon);
                        }
                        info!(target:"messages::oms::out::tdgw","conn_id={:?},msg={}",conn_id,oms_logon);
                    }
                }
            }
            SessionStatus::LoggedIn | SessionStatus::Ready => {
                //duplicate logon
                warn!(target: "business", "duplicate logon,conn_id={:?},conn_tag={:?},conn_status={:?}",conn_id,session.conn_tag,cur_statu);
                let mut logout = tdgw_bin::logout::Logout::new();
                logout.set_text_from_string("duplicate logon");
                logout.filled_head_and_tail();
                match session
                    .conn
                    .tcp_conn_send_bytes(&logout.as_bytes_big_endian())
                {
                    Ok(_) => {
                        info!(target: "business", "duplicate logon: logout sent, conn_id={:?}", conn_id);
                    }
                    Err(error) => {
                        error!(target: "business", "duplicate logon: send logout fail, conn_id={:?}, error={:?}", 
                               conn_id, error);
                    }
                }
                session.status = SessionStatus::WaitDisconnect;
            }
            _ => {
                warn!(
                    "get oms logon in error session state,conn_id={:?},session state={:?}",
                    conn_id, session.status
                );
            }
        }
        result
    }

    //收到oms_logon -> 回复logon,status=LoggedIn -> 回复平台状态信息 ->回复execrptinfo
    pub fn process_oms_logon_msg_tgw(
        &mut self,
        now: u128,
        conn_id: u16,
        oms_logon: &tgw_bin::logon::Logon,
    ) -> bool {
        let mut result = false;
        let session = self.conn_id_2_session.get_mut(&conn_id).unwrap();
        let cur_statu = session.status.clone();
        let conn_info = session.conn.tcp_get_conn_info();
        let remote_ip = conn_info.get_remote_ip();
        if !OMSCONFIG.is_ip_allowed(&remote_ip) {
            warn!(target: "business", "IP not in whitelist: conn_id={:?}, remote_ip={}, rejecting login", 
                  conn_id, remote_ip);
            let mut logout = tdgw_bin::logout::Logout::new();
            logout.set_text_from_string("IP address not in whitelist");
            logout.filled_head_and_tail();
            match session
                .conn
                .tcp_conn_send_bytes(&logout.as_bytes_big_endian())
            {
                Ok(_) => {
                    info!(target: "business", "IP whitelist rejection: logout sent, conn_id={:?}, remote_ip={}", 
                          conn_id, remote_ip);
                }
                Err(error) => {
                    error!(target: "business", "IP whitelist rejection: send logout fail, conn_id={:?}, error={:?}", 
                           conn_id, error);
                }
            }
            session.status = SessionStatus::WaitDisconnect;
            return false;
        }
        info!(target: "business", "IP whitelist check passed: conn_id={:?}, remote_ip={}", conn_id, remote_ip);
        match cur_statu {
            SessionStatus::Connected => {
                //reply logon
                match session
                    .conn
                    .tcp_conn_send_bytes(&oms_logon.as_bytes_big_endian())
                {
                    Err(error) => {
                        error!(
                            "send fail: conn_id={:?},error{:?},msg={}",
                            conn_id, error, oms_logon
                        );
                        session.status = SessionStatus::WaitDisconnect;
                    }
                    Ok(_) => {
                        session.status = SessionStatus::LoggedIn;
                        session.last_write_time_ms = now;
                        result = true;
                        if oms_logon.get_heart_bt_int() > 0 {
                            session.heart_beat = oms_logon.get_heart_bt_int();
                        } else {
                            warn!(target: "business", "oms logon, hearbeat value invalid: conn_id={:?},msg={}", conn_id, oms_logon);
                        }
                        info!(target:"messages::oms::out::tgw","conn_id={:?},msg={}",conn_id,oms_logon);
                    }
                }
            }
            SessionStatus::LoggedIn | SessionStatus::Ready => {
                //duplicate logon
                warn!(target: "business", "duplicate logon,conn_id={:?},conn_tag={:?},conn_status={:?}",conn_id,session.conn_tag,cur_statu);
                let mut logout = tdgw_bin::logout::Logout::new();
                logout.set_text_from_string("duplicate logon");
                logout.filled_head_and_tail();
                match session
                    .conn
                    .tcp_conn_send_bytes(&logout.as_bytes_big_endian())
                {
                    Ok(_) => {
                        info!(target: "business", "duplicate logon: logout sent, conn_id={:?}", conn_id);
                    }
                    Err(error) => {
                        error!(target: "business", "duplicate logon: send logout fail, conn_id={:?}, error={:?}", 
                               conn_id, error);
                    }
                }
                session.status = SessionStatus::WaitDisconnect;
            }
            _ => {
                warn!(
                    "get oms logon in error session state,conn_id={:?},session state={:?}",
                    conn_id, session.status
                );
            }
        }
        result
    }

    //处理需要关闭的连接 -> 将关闭成功的gw_session其加入重连列表 -> 关闭成功后更新网关路由表
    pub fn process_wait_disconnect_event(&mut self, now: u128) -> bool {
        let mut has_wait_disconnect_gw = false;
        for (conn_id, session) in &mut self.conn_id_2_session {
            let cur_states = session.status.clone();
            match cur_states {
                SessionStatus::WaitDisconnect => {
                    //待确认，更新网关路由表，（1）更新条件,网关侧连接进入WaitDisconnect状态 （2）更新路由表需要原子化，考虑加锁
                    if session.conn_type == ConnType::CLIENT {
                        has_wait_disconnect_gw = true;
                    }
                    //send logout
                    #[cfg(feature = "tdgw")]
                    {
                        let mut logout = tdgw_bin::logout::Logout::new();
                        //todo 考虑在session里记录一下关闭reason
                        logout.set_text_from_string("close by share offer");
                        logout.filled_head_and_tail();
                        match session
                            .conn
                            .tcp_conn_send_bytes(&logout.as_bytes_big_endian())
                        {
                            Ok(_) => {
                                info!(target: "system", "process_wait_disconnect_event, send logout success,conn_id={:?}",conn_id);
                            }
                            Err(error) => {
                                warn!(target: "system", "process_wait_disconnect_event, send logout fail: conn_id={:?},error={:?}",conn_id, error);
                                continue;
                            }
                        }
                    }

                    //close tcp conn , add gw_session to reconnect list
                    //todo :确认tcp_conn_close函数处理结束 和 收到closing、closed事件的时间顺序
                    match session.conn.tcp_conn_close() {
                        Ok(_) => {
                            session.status = SessionStatus::Closing;
                            if session.conn_type == ConnType::CLIENT {
                                self.session_to_reconnect.push(session.conn_id);
                            }
                            info!(target: "system", "process_wait_disconnect_event, close conn success, time={:?}, conn_id={:?},session_status={:?}",now,conn_id,session.status);
                        }
                        Err(error) => {
                            warn!(target: "system", "process_wait_disconnect_event, close conn fail:,conn_id={:?}, {:?}", conn_id, error);
                        }
                    }
                }
                _ => {}
            }
        }
        has_wait_disconnect_gw
    }

    pub fn process_session_reconnect_event(
        &mut self,
        now: u128,
        g_mgr: *mut tcp_connection::tcp_conn_manage_t,
        pipe_epoll_fd: &mut tcp_connection::TCPEventEpoll,
    ) {
        let mut reconnect_sessions_new = vec![];
        for conn_id in &self.session_to_reconnect {
            match self.conn_id_2_session.get(&conn_id) {
                None => {
                    warn!(
                        "process_session_reconnect_event, should not happen,conn_id={:?}",
                        conn_id
                    );
                }
                Some(session) => {
                    //重连间隔检查
                    if now - session.last_read_time_ms
                        > (session.reconnect_interval as u128 * 1000 * 1000 * 1000)
                    {
                        debug!(
                            "process_session_reconnect_event, reconnecting: {:?}, now: {}, last_read_time: {}",
                            session, now, session.last_read_time_ms
                        );
                        let session_type = session.session_type.clone();
                        match session_type {
                            SessionType::OMS => {
                                //判断网关状态(至少有一个处于ready)，启动柜台侧的监听
                                let ready_gw_num = self.get_ready_gw_conn_ids().len();
                                // info!(target:"system",
                                //     "process_session_reconnect_event, ready_gw_num={:?}, conn_id={:?}, status={:?}",
                                //     ready_gw_num, conn_id, session.status
                                // );
                                if ready_gw_num > 0 && session.status == SessionStatus::Disconnected
                                {
                                    unsafe {
                                        let conn = tcp_conn_find_by_id(g_mgr, session.conn_id);
                                        let ret = tcp_conn_listen(conn);
                                        info!(target: "system","process_session_reconnect_event, start listen,conn_id={}, conn_tag={}, ret:{}",session.conn_id, session.conn_tag, ret);
                                        if ret < 0 {
                                            reconnect_sessions_new.push(session.conn_id);
                                        } else {
                                            //todo:add error handle
                                            pipe_epoll_fd
                                                .add_connection(&TCPConnection { conn })
                                                .unwrap();
                                        }
                                    }
                                } else {
                                    reconnect_sessions_new.push(session.conn_id);
                                }
                            }
                            _ => {
                                //重新建立网关侧连接
                                {
                                    if session.status == SessionStatus::Disconnected {
                                        unsafe {
                                            let conn = tcp_conn_find_by_id(g_mgr, session.conn_id);
                                            let ret = tcp_conn_connect(conn);
                                            info!(target: "system","process_session_reconnect_event, gw connect, conn_id={}, conn_tag={}, ret:{}",session.conn_id, session.conn_tag, ret);
                                            if ret < 0 {
                                                reconnect_sessions_new.push(session.conn_id);
                                            } else {
                                                //todo:add error handle
                                                pipe_epoll_fd
                                                    .add_connection(&TCPConnection { conn })
                                                    .unwrap();
                                            }
                                        }
                                    } else {
                                        reconnect_sessions_new.push(session.conn_id);
                                    }
                                }
                            }
                        }
                    } else {
                        reconnect_sessions_new.push(session.conn_id);
                    }
                }
            }
        }
        self.session_to_reconnect = reconnect_sessions_new;
    }

    //中间件通知: tcp连接已关闭、资源也重置完毕
    pub fn process_tcp_conn_closed_event(&mut self, now: u128, conn_id: u16) -> bool{
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => {
                warn!(target: "system","process tcp closed event:should not happen,conn_id={:?}",conn_id);
            }
            Some(session) => match session.status {
                SessionStatus::Disconnected => {
                    warn!(target: "system","process tcp closed event:session is already disconnected,conn_id={:?}",conn_id);  
                }
                _ => {
                    info!(target: "system","process tcp closed event:session closed,pre_state={:?},conn_id={:?}",session.status,conn_id);
                    session.status = SessionStatus::Disconnected;
                    return true
                }
            },
        }
        false
    }

    pub fn process_tcp_conn_closing_event(&mut self, now: u128, conn_id: u16) {
        match self.conn_id_2_session.get_mut(&conn_id) {
            None => {
                warn!(target: "system","process tcp closing event:should not happen,conn_id={:?}",conn_id);
            }
            Some(session)=> {
                info!(target: "system","get tcp closing event,conn_id={:?}, session status={:?}, now={:?}",conn_id,session.status,now);
                if session.status != SessionStatus::Closing && session.status != SessionStatus::Disconnected {
                    session.status = SessionStatus::Closing;
                }
            }
        }
    }

    /* 
    pub fn process_tcp_conn_error_event(&self, now: u128, conn_id: u16, error_code: u8) {
        match self.conn_id_2_session.get(&conn_id) {
            None => {
                warn!(target: "system","process tcp error event:should not happen,conn_id={:?}",conn_id);
            }
            Some(session) => {
                info!(target:
                    "system","get tcp error event:,conn_id={:?},error_code={:?},error_msg={:?},session_state={:?},now={:?}",
                    conn_id,
                    error_code,
                    Self::get_tcp_error(error_code as i32),
                    session.status,
                    now
                );
                // todo: add error handle
            }
        }
    }*/

    pub fn get_tcp_error(err: i32) -> String {
        unsafe {
            let c_str = tcp_conn_strerror(err);
            if c_str.is_null() {
                "Unknown error".to_string()
            } else {
                std::ffi::CStr::from_ptr(c_str)
                    .to_string_lossy()
                    .into_owned()
            }
        }
    }

    pub fn tdgw_send_execrptinfo_for_test(
        &mut self,
        now: u128,
        oms_conn_id: u16,
    ) -> Result<(), String> {
        let session = self.conn_id_2_session.get_mut(&oms_conn_id).unwrap();
        let mut exec_rpt_info = tdgw_bin::exec_rpt_info::ExecRptInfo::new();
        let tdgw_info = match **&session.detail_config {
            DetailConfig::OMSINFO(ref tdgw_detail) => Ok(tdgw_detail),
            _ => Err(format!(
                "unreachalbe!,Connection detail not found!,connd_id={:?},config={:?}",
                oms_conn_id, &session.detail_config
            )),
        }?;
        exec_rpt_info.set_platform_id(tdgw_info.platform_id);
        let mut no_group_4 = tdgw_bin::exec_rpt_info::NoGroups4::default();
        //用于通过柜台代码检查，实际pbu柜台后续会从redis中获取
        no_group_4.set_pbu_from_string("99999999");
        let mut v_no_group_4 = vec![];
        v_no_group_4.push(no_group_4);
        exec_rpt_info.set_no_groups_4(&v_no_group_4);
        exec_rpt_info.filled_head_and_tail();
        match session.conn.tcp_conn_send_bytes(&exec_rpt_info.as_bytes()) {
            Ok(_) => {
                info!(target: "messages::share_offer::out::tdgw", "conn_id={:?},time={:?},msg ={:?}",oms_conn_id,now,exec_rpt_info);
                Ok(())
            }
            Err(error) => {
                warn!(target: "system", "tdgw_send_execrptinfo_for_test, send execrptinfo fail: conn_id={:?},{:?}",oms_conn_id, error);
                Err(error.to_string())
            }
        }
    }

    //同时记录seen_platform_ids
    pub fn build_gw_status_event(
        &mut self,
        conn_id: u16,
        platform_id: u16,
        platform_state: u16,
    ) -> Option<RedisWriteEvent> {
        let session = self.conn_id_2_session.get(&conn_id)?;
        let pbus = match &*session.detail_config {
            DetailConfig::TDGWINFO(cfg) => cfg.pbus.join(","),
            _ => String::new(),
        };
        self.seen_platform_ids
            .entry(conn_id)
            .or_default()
            .insert(platform_id);
        let updated_at = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        Some(RedisWriteEvent::GwStatus(GwStatusEvent {
            share_offer_id: TCPSHARECONFIG.share_offer_id,
            route_id: session.route_id,
            platform_id,
            platform_state: platform_state as i32,
            pbus,
            updated_at,
        }))
    }

    pub fn build_gw_disconnect_events(&self, conn_id: u16) -> Vec<RedisWriteEvent> {
        let session = match self.conn_id_2_session.get(&conn_id) {
            Some(s) if s.session_type == SessionType::TDGW => s,
            _ => return vec![],
        };
        let platform_ids = match self.seen_platform_ids.get(&conn_id) {
            Some(ids) => ids.clone(),
            None => return vec![],
        };
        let pbus = match &*session.detail_config {
            DetailConfig::TDGWINFO(cfg) => cfg.pbus.join(","),
            _ => String::new(),
        };
        let updated_at = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        platform_ids
            .iter()
            .map(|&pid| RedisWriteEvent::GwStatus(GwStatusEvent {
                share_offer_id: TCPSHARECONFIG.share_offer_id,
                route_id: session.route_id,
                platform_id: pid,
                platform_state: -1,
                pbus: pbus.clone(),
                updated_at: updated_at.clone(),
            }))
            .collect()
    }

    pub fn build_gw_list_events(&self) -> Vec<RedisWriteEvent> {
        let mut platform_gwids: std::collections::HashMap<u16, Vec<String>> = std::collections::HashMap::new();
        for session in self.conn_id_2_session.values() {
            if session.session_type != SessionType::TDGW {
                continue;
            }
            if let DetailConfig::TDGWINFO(cfg) = &*session.detail_config {
                let abs_gw_id = format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + session.route_id as u32);
                platform_gwids.entry(cfg.platform_id).or_default().push(abs_gw_id);
            }
        }
        platform_gwids.into_iter().map(|(platform_id, gwids)| {
            RedisWriteEvent::GwList(GwListEvent {
                share_offer_id: TCPSHARECONFIG.share_offer_id,
                platform_id,
                gwids,
            })
        }).collect()
    }

    pub fn build_gw_info_event(&self, conn_id: u16) -> Option<RedisWriteEvent> {
        let session = self.conn_id_2_session.get(&conn_id)?;
        if session.session_type != SessionType::TDGW {
            return None;
        }
        let (sender_comp_id, pbus, platform_id) = match &*session.detail_config {
            DetailConfig::TDGWINFO(cfg) => (
                cfg.sender_comp_id.clone(),
                cfg.pbus.clone(),
                cfg.platform_id,
            ),
            _ => return None,
        };
        let abs_gw_id = format!("{}", TCPSHARECONFIG.share_offer_id as u32 * 100 + session.route_id as u32);
        Some(RedisWriteEvent::GwInfo(GwInfoEvent {
            gwid: abs_gw_id,
            share_offer_id: TCPSHARECONFIG.share_offer_id,
            route_id: session.route_id,
            sender_comp_id,
            pbus,
            platform_id,
        }))
    }

    pub fn tgw_send_report_synchronization_for_test(
        &mut self,
        now: u128,
        oms_conn_id: u16,
        partations: &Vec<i32>
    ) -> Result<(), String> {
        let session = self.conn_id_2_session.get_mut(&oms_conn_id).unwrap();
        let mut report_synchronization =
            tgw_bin::report_synchronization::ReportSynchronization::new();
        let mut v_no_partition_2 = vec![];
        for each_partation in partations{
            let mut no_partition_2 = tgw_bin::report_synchronization::NoPartitions2::default();
            //用于通过柜台代码检查，实际pbu柜台后续会从redis中获取
            no_partition_2.set_partition_no(*each_partation);
            no_partition_2.set_report_index(1);
            v_no_partition_2.push(no_partition_2);
        }
        report_synchronization.set_no_partitions_2(&v_no_partition_2);
        report_synchronization.filled_head_and_tail();
        match session
            .conn
            .tcp_conn_send_bytes(&report_synchronization.as_bytes())
        {
            Ok(_) => {
                info!(target: "messages::share_offer::out::tgw", "conn_id={:?},time={:?},msg ={:?}",oms_conn_id,now,report_synchronization);
                Ok(())
            }
            Err(error) => {
                warn!(target: "system", "tgw tgw_send_report_synchronization_for_test, send report_synchronization fail: conn_id={:?},{:?}",oms_conn_id, error);
                Err(error.to_string())
            }
        }
    }


    pub fn tgw_send_plateform_info_for_test(
        &mut self,
        now: u128,
        oms_conn_id: u16,
        partations: &Vec<i32>
    ) -> Result<(), String> {
        let session = self.conn_id_2_session.get_mut(&oms_conn_id).unwrap();
        let mut plateform_info =
            tgw_bin::platform_info::PlatformInfo::new();
        let mut v_no_partition_3 = vec![];
        for each_partation in partations{
            let mut no_partition_3 = tgw_bin::platform_info::NoPartitions3::default();
            //用于通过柜台代码检查，实际pbu柜台后续会从redis中获取
            no_partition_3.set_partition_no(*each_partation);
        }
        plateform_info.set_no_partitions_3(&v_no_partition_3);
        plateform_info.filled_head_and_tail();
        match session
            .conn
            .tcp_conn_send_bytes(&plateform_info.as_bytes())
        {
            Ok(_) => {
                info!(target: "messages::share_offer::out::tgw", "conn_id={:?},time={:?},msg ={:?}",oms_conn_id,now,plateform_info);
                Ok(())
            }
            Err(error) => {
                warn!(target: "system", "tgw tgw_send_plateform_info_for_test, send report_synchronization fail: conn_id={:?},{:?}",oms_conn_id, error);
                Err(error.to_string())
            }
        }
    }
}
