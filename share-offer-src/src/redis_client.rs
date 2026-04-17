use redis::{Commands, RedisError, cluster::ClusterClient, cluster::ClusterConnection};
use std::collections::HashMap;
use std::time::Duration;
use std::thread;
use crossbeam_channel::{Receiver, RecvTimeoutError};
use tracing::{info, warn, error, debug};
use crate::constants::IdMapType;

pub struct RedisClient {
    cluster_client: ClusterClient,
}

impl RedisClient {
    pub fn new_cluster(nodes: Vec<String>) -> Result<Self, RedisError> {
        let cluster_client = ClusterClient::builder(nodes.clone())
            .connection_timeout(Duration::from_secs(5))
            .response_timeout(Duration::from_secs(5))
            .build()?;
        info!(target: "system", "Redis cluster client created: nodes={:?}", nodes);
        Ok(Self { cluster_client })
    }

    pub fn get_connection(&self) -> Result<ClusterConnection, RedisError> {
        self.cluster_client.get_connection()
    }

    pub fn get_max_report_index(
        &self,
        gw_id: &str,
        pbu: &str,
        set_id: u32
    ) -> Result<u64, RedisError> {
        let key = format!("share_offer_max_reportIndex_{}_{}_{}",
                          gw_id, pbu, set_id);
        let mut conn = self.get_connection()?;
        match conn.get::<_, Option<u64>>(&key) {
            Ok(Some(index)) => {
                info!(target: "business", "Redis get_max_report_index: key={}, index={}", key, index);
                Ok(index)
            }
            Ok(None) => {
                info!(target: "business", "Redis get_max_report_index: key={}, not found, return 0", key);
                Ok(0)
            }
            Err(e) => {
                error!(target: "business", "Redis get_max_report_index failed: key={}, error={:?}", key, e);
                Err(e)
            }
        }
    }

    pub fn set_max_report_index(
        &self,
        gw_id: &str,
        pbu: &str,
        set_id: u32,
        index: u64
    ) -> Result<(), RedisError> {
        let key = format!("share_offer_max_reportIndex_{}_{}_{}",
                          gw_id, pbu, set_id);
        let ttl_seconds = 10 * 60 * 60; // 10 hours
        let mut conn = self.get_connection()?;
        let _: () = conn.set_ex(&key, index, ttl_seconds)?;
        info!(target: "business", "Redis set_max_report_index: key={}, index={}, TTL=10h", key, index);
        Ok(())
    }

    pub fn batch_get_max_report_index(
        &self,
        gw_id: &str,
        pbu_set_pairs: &[(String, u32)]
    ) -> Result<HashMap<(String, u32), u64>, RedisError> {
        let mut conn = self.get_connection()?;
        let mut result = HashMap::new();
        for (pbu, set_id) in pbu_set_pairs {
            let key = format!("share_offer_max_reportIndex_{}_{}_{}",
                              gw_id, pbu, set_id);
            match conn.get::<_, Option<u64>>(&key) {
                Ok(Some(index)) => {
                    result.insert((pbu.clone(), *set_id), index);
                }
                Ok(None) => {
                    result.insert((pbu.clone(), *set_id), 0);
                }
                Err(e) => {
                    warn!(target: "business", "Redis batch_get: key={}, error={:?}", key, e);
                    result.insert((pbu.clone(), *set_id), 0);
                }
            }
        }
        
        info!(target: "business", "Redis batch_get_max_report_index: {} keys fetched", result.len());
        Ok(result)
    }


    pub fn set_partition_routing(
        &self,
        share_offer_id: u16,
        pbu: &str,
        set_id: u32,
        route_id: u16,
    ) -> Result<(), RedisError> {
        let key = format!("share_offer_{}_routing_{}_{}", share_offer_id, pbu, set_id);
        let mut conn = self.get_connection()?;
        let _: () = conn.set(&key, route_id)?;
        info!(target: "business", "Redis set_partition_routing: key={}, route_id={}", key, route_id);
        Ok(())
    }

    pub fn get_partition_routing(
        &self,
        share_offer_id: u16,
        pbu: &str,
        set_id: u32,
    ) -> Result<Option<u16>, RedisError> {
        let key = format!("share_offer_{}_routing_{}_{}", share_offer_id, pbu, set_id);
        let mut conn = self.get_connection()?;
        match conn.get::<_, Option<u16>>(&key) {
            Ok(route_id) => {
                info!(target: "business", "Redis get_partition_routing: key={}, route_id={:?}", key, route_id);
                Ok(route_id)
            }
            Err(e) => {
                error!(target: "business", "Redis get_partition_routing failed: key={}, error={:?}", key, e);
                Err(e)
            }
        }
    }

    pub fn remove_partition_routing(
        &self,
        share_offer_id: u16,
        pbu: &str,
        set_id: u32,
    ) -> Result<(), RedisError> {
        let key = format!("share_offer_{}_routing_{}_{}", share_offer_id, pbu, set_id);
        let mut conn = self.get_connection()?;
        let _: () = conn.del(&key)?;
        info!(target: "business", "Redis remove_partition_routing: key={}", key);
        Ok(())
    }

    pub fn ping(&self) -> Result<(), RedisError> {
        let mut conn = self.get_connection()?;
        let _: String = redis::cmd("PING").query(&mut conn)?;
        info!(target: "system", "Redis ping success");
        Ok(())
    }

    pub fn batch_get_execution_reports(
        &self,
        server_id: &str,
        gw_id: &str,
        pbu: &str,
        set_id: u32,
        begin_index: u64,
        latest_index: u64,
    ) -> Result<Vec<(u64, Vec<u8>)>, RedisError> {
        let key = format!("share_offer_exec_rpt_{}_{}_{}_{}" ,
                          server_id, gw_id, pbu, set_id);
        let mut conn = self.get_connection()?;
        let members: Vec<(Vec<u8>, f64)> = conn.zrangebyscore_withscores(
            &key, begin_index as f64, latest_index as f64
        )?;
        let reports: Vec<(u64, Vec<u8>)> = members.into_iter()
            .map(|(data, score)| (score as u64, data))
            .collect();
        info!(target: "business",
              "Redis batch_get_execution_reports: key={}, range={}..{}, fetched={}",
              key, begin_index, latest_index, reports.len());
        Ok(reports)
    }

    pub fn hset_id_mapping(
        &self,
        map_type: IdMapType,
        share_id: &str,
        field: &str,
        value: &str
    ) -> Result<(), RedisError> {
        let key = format!("share_offer_id_map_{}_{}", map_type.as_str(), share_id);
        let mut conn = self.get_connection()?;

        let _: () = conn.hset(&key, field, value)?;
        #[cfg(debug_assertions)]
        debug!(target: "business", "Redis HSET: key={}, field={}, value={}", key, field, value);
        Ok(())
    }


    pub fn store_execution_report(
        &self,
        server_id: &str,
        gw_id: &str,
        pbu: &str,
        partition_no: u32,
        report_index: u64,
        report_data: &[u8]
    ) -> Result<(), RedisError> {
        let key = format!("share_offer_exec_rpt_{}_{}_{}_{}" ,
                          server_id, gw_id, pbu, partition_no);
        let ttl_seconds = 10 * 60 * 60; // 10 hours
        let mut conn = self.get_connection()?;

        let _: () = conn.zadd(&key, report_data, report_index)?;
        let _: () = conn.expire(&key, ttl_seconds)?;

        info!(target: "business",
              "Redis store_execution_report: server_id={}, gw_id={}, pbu={}, partition_no={}, report_index={}, size={} bytes",
              server_id, gw_id, pbu, partition_no, report_index, report_data.len());
        Ok(())
    }

}

pub struct ExecReportEvent {
    pub server_id: String,
    pub share_offer_id: u16,
    pub route_id: u16,
    pub gw_id: String,
    pub platform_id: u16,
    pub pbu: String,
    pub partition_no: u32,
    pub report_index: u64,
    pub report_data: Vec<u8>,
}

pub struct GwStatusEvent {
    pub share_offer_id: u16,
    pub route_id: u16,
    pub platform_id: u16,
    pub platform_state: i32, // -1=DISCONNECT, 0=PREOPEN, 1=OPENUPCOMING, 2=OPEN, 3=HALT, 4=CLOSE
    pub pbus: String,
    pub updated_at: String,
}

pub struct GwListEvent {
    pub share_offer_id: u16,
    pub platform_id: u16,
    pub gwids: Vec<String>,
}

pub struct GwInfoEvent {
    pub gwid: String,
    pub share_offer_id: u16,
    pub route_id: u16,
    pub sender_comp_id: String,
    pub pbus: Vec<String>,
    pub platform_id: u16,
}

pub enum RedisWriteEvent {
    ExecReport(ExecReportEvent),
    GwStatus(GwStatusEvent),
    GwList(GwListEvent),
    GwInfo(GwInfoEvent),
}

pub fn store_event_pipeline(
    conn: &mut ClusterConnection,
    event: &ExecReportEvent,
) -> Result<(), RedisError> {
    let ttl: usize = 10 * 60 * 60; // 10 hours
    let report_key = format!(
        "share_offer_flash_report_{}_{}_{}_{}_{}" ,
        event.server_id, event.gw_id, event.platform_id, event.pbu, event.partition_no
    );
    let max_index_key = format!(
        "share_offer_max_reportIndex_{}_{}_{}" ,
        event.gw_id, event.pbu, event.partition_no
    );
    let routing_key = format!(
        "share_offer_{}_routing_{}_{}",
        event.share_offer_id, event.pbu, event.partition_no
    );
    let known_partitions_key = format!(
        "share_offer_{}_known_setids_{}",
        event.share_offer_id, event.route_id
    );
    let partition_member = format!("{}:{}", event.pbu, event.partition_no);

    debug!(target: "redis", "Redis store_event_pipeline: report_key={}, max_index_key={}, routing_key={}, report_index={}",
        report_key, max_index_key, routing_key, event.report_index);

    let _: i32 = conn.zadd(&report_key, event.report_data.as_slice(), event.report_index)?;
    let _: i32 = conn.expire(&report_key, ttl as i64)?;
    let _: () = conn.set_ex(&max_index_key, event.report_index, ttl as u64)?;
    let _: () = conn.set(&routing_key, event.route_id)?;
    let _: i32 = conn.sadd(&known_partitions_key, &partition_member)?;
    let _: bool = conn.expire(&known_partitions_key, ttl as i64)?;

    Ok(())
}

pub fn start_redis_write_thread(
    rx: Receiver<RedisWriteEvent>,
    nodes: Vec<String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let client = match RedisClient::new_cluster(nodes) {
            Ok(c) => c,
            Err(e) => {
                error!(target: "system", "Redis write thread: failed to create client: {:?}", e);
                return;
            }
        };
        let mut conn = match client.get_connection() {
            Ok(c) => c,
            Err(e) => {
                error!(target: "system", "Redis write thread: failed to get connection: {:?}", e);
                return;
            }
        };
        match redis::cmd("PING").query::<String>(&mut conn) {
            Ok(resp) => info!(target: "system", "Redis write thread: initial PING ok, resp={}", resp),
            Err(e)  => error!(target: "system", "Redis write thread: initial PING failed: {:?}", e),
        };
        const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
        info!(target: "system", "Redis write thread started (pipeline mode)");
        loop {
            match rx.recv_timeout(KEEPALIVE_INTERVAL) {
                Ok(RedisWriteEvent::ExecReport(event)) => {
                    if let Err(e) = store_event_pipeline(&mut conn, &event) {
                        error!(target: "business", "Redis write thread: pipeline failed: {:?}, retrying after reconnect", e);
                        match client.get_connection() {
                            Ok(new_conn) => {
                                conn = new_conn;
                                info!(target: "system", "Redis write thread: reconnected");
                                if let Err(re) = store_event_pipeline(&mut conn, &event) {
                                    error!(target: "business", "Redis write thread: retry failed: {:?}, dropped: pbu={}, partition_no={}, report_index={}",
                                        re, event.pbu, event.partition_no, event.report_index);
                                } else {
                                    info!(target: "business", "Redis write thread: retry ok: pbu={}, partition_no={}, report_index={}",
                                        event.pbu, event.partition_no, event.report_index);
                                }
                            }
                            Err(re) => {
                                error!(target: "system", "Redis write thread: reconnect failed: {:?}, dropped: pbu={}, partition_no={}, report_index={}",
                                    re, event.pbu, event.partition_no, event.report_index);
                            }
                        }
                    }
                }
                Ok(RedisWriteEvent::GwStatus(ev)) => {
                    let key = format!(
                        "share_offer_{}_tdgw_platform_{}_{}",
                        ev.share_offer_id, ev.route_id, ev.platform_id
                    );
                    let write_result = (|| -> Result<(), RedisError> {
                        let _: () = conn.hset(&key, "platform_state", ev.platform_state)?;
                        let _: () = conn.hset(&key, "pbus", &ev.pbus)?;
                        let _: () = conn.hset(&key, "updated_at", &ev.updated_at)?;
                        Ok(())
                    })();
                    match write_result {
                        Ok(_) => debug!(target: "redis", "Redis gw_status ok: key={}, state={}", key, ev.platform_state),
                        Err(e) => warn!(target: "business", "Redis gw_status failed: key={}, err={:?}", key, e),
                    }
                }
                Ok(RedisWriteEvent::GwList(ev)) => {
                    let key = format!("share_offer_{}_gw_list_{}", ev.share_offer_id, ev.platform_id);
                    let value = ev.gwids.join(",");
                    match conn.set::<_, _, ()>(&key, &value) {
                        Ok(_) => debug!(target: "redis", "Redis gw_list ok: key={}, value={}", key, value),
                        Err(e) => warn!(target: "business", "Redis gw_list failed: key={}, err={:?}", key, e),
                    }
                }
                Ok(RedisWriteEvent::GwInfo(ev)) => {
                    let key = format!("share_offer_{}_gw_info_{}", ev.share_offer_id, ev.route_id);
                    let pbus_json = ev.pbus.iter()
                        .map(|p| format!("\"{}\"", p))
                        .collect::<Vec<_>>()
                        .join(",");
                    let value = format!(
                        "{{\"gwid\":\"{}\",\"share_offer_id\":{},\"route_id\":{},\"sender_comp_id\":\"{}\",\"pbus\":[{}],\"platform_id\":{}}}",
                        ev.gwid, ev.share_offer_id, ev.route_id, ev.sender_comp_id, pbus_json, ev.platform_id
                    );
                    match conn.set::<_, _, ()>(&key, &value) {
                        Ok(_) => debug!(target: "redis", "Redis gw_info ok: key={}", key),
                        Err(e) => warn!(target: "business", "Redis gw_info failed: key={}, err={:?}", key, e),
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    let ping_result: Result<String, _> = redis::cmd("PING").query(&mut conn);
                    if let Err(e) = ping_result {
                        warn!(target: "system", "Redis keepalive PING failed: {:?}, reconnecting", e);
                        match client.get_connection() {
                            Ok(new_conn) => {
                                conn = new_conn;
                                info!(target: "system", "Redis write thread: reconnected after keepalive failure");
                            }
                            Err(re) => {
                                error!(target: "system", "Redis write thread: reconnect failed: {:?}", re);
                            }
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    info!(target: "system", "Redis write thread: channel closed, exiting");
                    break;
                }
            }
        }
    })
}
