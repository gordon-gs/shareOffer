use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};
use fproto::stream_frame::tdgw_bin::TdgwBinFrame;
use fproto::stream_frame::tgw_bin::TgwBinFrame;
use std::{ptr, thread};
use std::{sync::Arc, time::Instant};
use tracing::{Level, debug, error, info, warn};
use tracing_subscriber::fmt::time;
use tracing_subscriber::{filter, prelude::*};
use std::time::Duration;

use crate::config::tcp_share::TCPSHARECONFIG;


pub enum MSGLOGENENT {
    InOmsTdgwMsgInfo(u128, u16, Arc<TdgwBinFrame>),
    InTdgwMsgInfo(u128, u16, Arc<TdgwBinFrame>),
    OutShareOfferTdgwMsgInfo(u128, u16, Arc<TdgwBinFrame>),
    BenchmarkInOMSTdgwInfo(usize,u128,u128, u16, Arc<TdgwBinFrame>),
    BenchmarkInTdgwInfo(usize,u128,u128, u16, Arc<TdgwBinFrame>),
    BenchmarkOutTgdwInfo(usize,u128, u16, Arc<TdgwBinFrame>),


    InOmsTgwMsgInfo(u128, u16, Arc<TgwBinFrame>),
    InTgwMsgInfo(u128, u16, Arc<TgwBinFrame>),
    OutShareOfferTgwMsgInfo(u128, u16, Arc<TgwBinFrame>),
    BenchmarkInOMSTgwInfo(usize,u128,u128, u16, Arc<TgwBinFrame>),
    BenchmarkInTgwInfo(usize,u128,u128, u16, Arc<TgwBinFrame>),
    BenchmarkOutTgwInfo(usize,u128, u16, Arc<TgwBinFrame>),

    DebugInfo(usize,String)
}

pub fn start_logging_thread(rx: Receiver<MSGLOGENENT>) -> thread::JoinHandle<()> {
    let core_ids = core_affinity::get_core_ids().unwrap();
    let bind_core = core_ids[core_ids.len()-1];
    let conn_id_to_route_id_config = TCPSHARECONFIG.conn_id_to_route_id_map.clone();
    thread::spawn(move || {
        // 【核心点】绑核：建议把日志线程绑在一个非业务核心上（如 Core 22）
        core_affinity::set_for_current(bind_core);
        loop {
            match rx.recv() {
                Ok(event) => match event {
                    MSGLOGENENT::InOmsTdgwMsgInfo(now, conn_id, msg) => {
                        info!(
                           target:"messages::oms::in::tdgw", "conn_id={:?},route_id={:?},time={:?},msg={}",
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            msg
                        );
                    }
                    MSGLOGENENT::InTdgwMsgInfo(now, conn_id, msg) => {
                        info!(
                           target:"messages::tdgw::in", "conn_id={:?},route_id={:?},time={:?},msg={}",
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            msg
                        );
                    }
                    MSGLOGENENT::OutShareOfferTdgwMsgInfo(now, conn_id, msg) => {
                        info!(
                           target:"messages::share_offer::out::tdgw", "conn_id={:?},route_id={:?},time={:?},msg={}",
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            msg
                        );
                    }
                    MSGLOGENENT::BenchmarkInOMSTdgwInfo(step, now, interval, conn_id, msg) => {
                        info!(
                           target:"benchmark::oms::in::message::tdgw", "step={:?},conn_id={:?},route_id={:?},time={:?},interval={:?},msg={}",
                            step,
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            interval,
                            msg
                        );
                    }
                    MSGLOGENENT::BenchmarkInTdgwInfo(step, now, interval, conn_id, msg) => {
                        info!(
                           target:"benchmark::tdgw::in::message", "step={:?},conn_id={:?},route_id={:?},time={:?},interval={:?},msg={}",
                            step,
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            interval,
                            msg
                        );
                    },
                     MSGLOGENENT::BenchmarkOutTgwInfo(step,intervel, conn_id, msg) => {
                        info!(
                           target:"benchmark::shareoffer::out::message::tgw", "step={:?},conn_id={:?},route_id={:?},time={:?},msg={}",
                            step,
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            intervel,
                            msg
                        );
                    },
                    MSGLOGENENT::InOmsTgwMsgInfo(now, conn_id, msg) => {
                        info!(
                           target:"messages::oms::in::tgw", "conn_id={:?},route_id={:?},time={:?},msg={}",
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            msg
                        );
                    }
                    MSGLOGENENT::InTgwMsgInfo(now, conn_id, msg) => {
                        info!(
                           target:"messages::tgw::in", "conn_id={:?},route_id={:?},time={:?},msg={}",
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            msg
                        );
                    }
                    MSGLOGENENT::OutShareOfferTgwMsgInfo(now, conn_id, msg) => {
                        info!(
                           target:"messages::share_offer::out::tgw", "conn_id={:?},route_id={:?},time={:?},msg={}",
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            msg
                        );
                    }
                    MSGLOGENENT::BenchmarkInOMSTgwInfo(step,now,interval, conn_id, msg) => {
                        info!(
                           target:"benchmark::oms::in::message::tgw", "step={:?},conn_id={:?},route_id={:?},time={:?},interval={:?},msg={}",
                            step,
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            interval,
                            msg
                        );
                    },
                    MSGLOGENENT::BenchmarkInTgwInfo(step,now,interval, conn_id, msg) => {
                        info!(
                           target:"benchmark::tgw::in::message", "step={:?},conn_id={:?},route_id={:?},time={:?},interval={:?},msg={}",
                            step,
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            now,
                            interval,
                            msg
                        );
                    },
                     MSGLOGENENT::BenchmarkOutTgdwInfo(step,intervel, conn_id, msg) => {
                        info!(
                           target:"benchmark::shareoffer::out::message::tdgw", "step={:?},conn_id={:?},route_id={:?},time={:?},msg={}",
                            step,
                            conn_id,
                            conn_id_to_route_id_config.get(&conn_id).unwrap_or(&999),
                            intervel,
                            msg
                        );
                    }
                    MSGLOGENENT::DebugInfo(step, info) => {
                        error!("step={:?},msg={}", step, info);
                    }
                },
                Err(e) => {
                    // 通道关闭（分发线程已停止发送消息），退出业务线程
                    error!("log thread:{} , 退出", e);
                    break;
                }
            }
            if cfg!(feature = "local_debug") {
                thread::sleep(Duration::from_millis(10));
            }
        }
    })
}
