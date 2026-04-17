use crate::config::tcp_share::TCPSHARECONFIG;
use crate::route::{RouteDirection, RouteInfo, RouteLinkType};
use crate::session::DetailConfig;
use crate::{constants, route, auto_reject};
use anyhow::Ok;
use fproto::FrameResult;
use fproto::protocol_error::ProtocolError;
use fproto::stream_frame::StreamFrame;
use fproto::stream_frame::tdgw_bin::TdgwBinFrame;
use fproto::stream_frame::tgw_bin::TgwBinFrame;

use crate::log::MSGLOGENENT;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use share_offer_sys::tcp_connection;
use std::collections::HashMap;
//use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Index;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tracing::{debug, error, info, warn};

pub struct MsgProcessor {
    pub routing_map: Vec<u16>,
    pub conn_id: u16,
    pub route_id: u16,
    pub route_direction: RouteDirection, //路由方向
    pub route_link_type: RouteLinkType,  //路由信息来源
    pub start_time: Instant,             //开始时间同步主线程
    pub mgr: Arc<tcp_connection::TCPConnectionManager>,
    pub msg_log_tx: Sender<MSGLOGENENT>,
    pub conn_id_to_route_id_map: HashMap<u16, u16>,
    pub route_id_to_conn_id_map: HashMap<u16, u16>,
}

pub enum MsgRxEvent {
    // routing map
    UpdateMap(Vec<u16>),
    // process msg
    NewTdgwOms2GwMsg(Arc<TdgwBinFrame>, Arc<DetailConfig>),
    NewTdgwGw2OmsMsg(Arc<TdgwBinFrame>, Arc<DetailConfig>),
    NewTgwOms2GwMsg(Arc<TgwBinFrame>, Arc<DetailConfig>),
    NewTgwGw2OmsMsg(Arc<TgwBinFrame>, Arc<DetailConfig>),
}

pub enum MsgTxResult {
    NewTgwMsg(Arc<TgwBinFrame>, u16),
    NewTdgwMsg(Arc<TdgwBinFrame>, u16),
    Disconnect(u16),
    //GWSendFail(u16)
}

macro_rules! msgProcessor_log_error {
    ($conn_id:expr, $($arg:tt)*) => {
        error!(
            "conn_id={}, MsgProcessor::business_thread::{}",
            $conn_id,
            format_args!($($arg)*)
        )
    };
}

impl MsgProcessor {
    /// 业务线程处理函数：接收消息 → 处理 → 发送结果
    /// rx: 业务线程的输入通道（接收分发线程的消息）
    /// tx_result: 结果通道的 Sender（发送处理结果到接收线程）
    pub fn business_thread(&mut self, rx: Receiver<MsgRxEvent>, tx_result: Sender<MsgTxResult>) {
        let mut last_update_time = self.start_time.elapsed().as_nanos();
        loop {
            //spin
            match rx.try_recv() {
                std::result::Result::Ok(MsgRxEvent::NewTdgwOms2GwMsg(msg, session_detail)) => {
                    //info!(target: "benchmark::msg_processor::NewTdgwOms2GwMsg::channel_recv::begin", "conn_id={:?},time={:?},msg={:?}",self.conn_id, self.start_time.elapsed().as_nanos(),msg);
                    match self.tdgw_handle_oms_2_gw_msg(&msg, &tx_result,session_detail) {
                        core::result::Result::Ok((target_conn, send_msg)) => {
                            let now = self.start_time.elapsed().as_nanos();
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgdwInfo(
                                1,
                                self.start_time.elapsed().as_nanos(),
                                target_conn,
                                Arc::clone(&send_msg),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    target_conn, e
                                );
                            }
                            if let Err(e) =
                                self.msg_log_tx.send(MSGLOGENENT::OutShareOfferTdgwMsgInfo(
                                    now,
                                    target_conn,
                                    Arc::clone(&send_msg),
                                ))
                            {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    target_conn, e
                                );
                            }
                            if (now - last_update_time) / 10000000 > 5 {
                                tx_result
                                    .send(MsgTxResult::NewTdgwMsg(send_msg, target_conn))
                                    .expect("连接管理线程发送失败");
                                last_update_time = now;
                            }
                        }
                        Err(ProtocolError::RouteNotFound) => {
                            //get or check route_info fail, disconnect oms session
                            if let Err(e) = tx_result.send(MsgTxResult::Disconnect(self.conn_id)) {
                                msgProcessor_log_error!(self.conn_id, "tdgw_handle_oms_2_gw_msg, route not found, send oms session Disconnect event fail: {:?}", e);
                            }
                        }
                        Err(ProtocolError::Skip) => {
                            //pass
                        }
                        Err(e) => {
                            msgProcessor_log_error!(self.conn_id, "tdgw_handle_oms_2_gw_msg, error: {:?}", e);
                        }
                    }
                }
                std::result::Result::Ok(MsgRxEvent::NewTdgwGw2OmsMsg(msg, session_detail)) => {
                    //info!(target: "benchmark::msg_processor::NewTdgwGw2OmsMsg::channel_recv", "conn_id={:?},time={:?},msg={:?}",self.conn_id, self.start_time.elapsed().as_nanos(),msg);
                    let route_info = match self
                        .tdgw_generate_route_info_from_msg(&msg, Arc::clone(&session_detail))
                    {
                        Some(info) => info,
                        None => {
                            error!(
                                "msg_processor::NewTdgwGw2OmsMsg::{:?},can't find route info from msg",
                                msg
                            );
                            return; // 继续处理下一条
                        }
                    };
                    if let Some(conn_id) = self.tdgw_gw_2_oms_route_check(&msg, &route_info) {
                        let id = *conn_id;
                        //Self::tdgw_record_2_redis(&msg);
                        match self.send_tdgw_msg(Arc::clone(&msg), id) {
                            core::result::Result::Ok(_) => {
                                let now = self.start_time.elapsed().as_nanos();
                                if let Err(e) =
                                    self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgdwInfo(
                                        1,
                                        self.start_time.elapsed().as_nanos(),
                                        id as u16,
                                        Arc::clone(&msg),
                                    ))
                                {
                                    error!(
                                        "send msg log error ,conid:{:?} error:{:?},close connection",
                                        id, e
                                    );
                                }
                                if let Err(e) =
                                    self.msg_log_tx.send(MSGLOGENENT::OutShareOfferTdgwMsgInfo(
                                        now,
                                        id,
                                        Arc::clone(&msg),
                                    ))
                                {
                                    error!(
                                        "send msg log error ,conid:{:?} error:{:?},close connection",
                                        id, e
                                    );
                                }
                                if (now - last_update_time) / 1000000 > 5 {
                                    tx_result
                                        .send(MsgTxResult::NewTdgwMsg(msg, id))
                                        .expect("连接管理线程发送失败");
                                    last_update_time = now;
                                }
                            }
                            Err(e) => {
                                msgProcessor_log_error!(self.conn_id, "tdgw_2_oms_send_error, msg={},error={}", msg, e);
                                //send msg to oms fail, disconnect oms session
                                if let Err(e) = tx_result.send(MsgTxResult::Disconnect(route_info.oms_id.clone())) {
                                    msgProcessor_log_error!(self.conn_id, "tdgw_2_oms, send oms session Disconnect event fail: {:?}",e);
                                }
                            }
                        }
                    } else {
                        msgProcessor_log_error!(self.conn_id, "tdgw_2_oms, userinfo error, can not route, msg={},route_info={:?}", msg, route_info);

                    }
                }
                std::result::Result::Ok(MsgRxEvent::NewTgwOms2GwMsg(msg, session_detail)) => {
                    match self.tgw_handle_oms_2_gw_msg(msg, &tx_result, session_detail) {
                        core::result::Result::Ok((target_conn, send_msg)) => {
                            let now = self.start_time.elapsed().as_nanos();
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgwInfo(
                                1,
                                self.start_time.elapsed().as_nanos(),
                                target_conn,
                                Arc::clone(&send_msg),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    target_conn, e
                                );
                            }
                            if let Err(e) =
                                self.msg_log_tx.send(MSGLOGENENT::OutShareOfferTgwMsgInfo(
                                    now,
                                    target_conn,
                                    Arc::clone(&send_msg),
                                ))
                            {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    target_conn, e
                                );
                            }
                            if (now - last_update_time) / 10000000 > 5 {
                                tx_result
                                    .send(MsgTxResult::NewTgwMsg(send_msg, target_conn))
                                    .expect("连接管理线程发送失败");
                                last_update_time = now;
                            }
                        }
                        Err(ProtocolError::Skip) => {
                            //pass
                        }
                        Err(e) => {
                            error!(
                                "MsgProcessor::business_thread::tgw_handle_oms_2_gw_msg::error:{}",
                                e
                            )
                        }
                    }
                }
                std::result::Result::Ok(MsgRxEvent::NewTgwGw2OmsMsg(msg, session_detail)) => {
                    let route_info = match self
                        .tgw_generate_route_info_from_msg(&msg, Arc::clone(&session_detail))
                    {
                        Some(info) => info,
                        None => {
                            error!(
                                "msg_processor::NewTgwGw2OmsMsg::{:?},can't find route info from msg",
                                msg
                            );
                            return; // 继续处理下一条
                        }
                    };
                    if let Some(conn_id) = self.tgw_gw_2_oms_route_check(&route_info) {
                        let id = *conn_id;
                        // todo redis record
                        match self.send_tgw_msg(Arc::clone(&msg), id) {
                            core::result::Result::Ok(_) => {
                                let now = self.start_time.elapsed().as_nanos();
                                if let Err(e) =
                                    self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgwInfo(
                                        1,
                                        self.start_time.elapsed().as_nanos(),
                                        id as u16,
                                        Arc::clone(&msg),
                                    ))
                                {
                                    error!(
                                        "send msg log error ,conid:{:?} error:{:?},close connection",
                                        id, e
                                    );
                                }
                                if let Err(e) = self.msg_log_tx.send(
                                    MSGLOGENENT::OutShareOfferTgwMsgInfo(now, id, Arc::clone(&msg)),
                                ) {
                                    error!(
                                        "send msg log error ,conid:{:?} error:{:?},close connection",
                                        id, e
                                    );
                                }
                                if (now - last_update_time) / 1000000 > 5 {
                                    tx_result
                                        .send(MsgTxResult::NewTgwMsg(msg, id))
                                        .expect("连接管理线程发送失败");
                                    last_update_time = now;
                                }
                            }
                            Err(e) => {
                                error!(
                                    "tgw 2 oms send error:conn_id={:?},msg={},error={}",
                                    id, msg, e
                                )
                            }
                        }
                    } else {
                        error!(
                            "msg_processor: {}, tgw 2 oms, userinfo error, can not route, msg:{},route_info:{:?}",
                            self.conn_id, msg, route_info
                        )
                    }
                }
                std::result::Result::Ok(MsgRxEvent::UpdateMap(map)) => {
                    info!(target:"system","msg_processor,conn_id={:?},update routing_map={:?}",self.conn_id,map);
                    self.routing_map = map;
                }
                Err(TryRecvError::Empty) => {
                    //loop 跑满这个核心
                }
                Err(TryRecvError::Disconnected) => {
                    // todo!(deal exception)
                    error!("msg_processor: {}, 退出", self.conn_id);
                    break;
                } // Err(_) => {
                  //     // 通道关闭（分发线程已停止发送消息），退出业务线程
                  //     error!("msg_processor: {}, 退出", self.conn_id);
                  //     break;
                  // }
            }
            if cfg!(feature = "local_debug") {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    /// 处理Tdgw OMS->GW方向消息
    fn tdgw_handle_oms_2_gw_msg(
        &mut self,
        msg: &Arc<TdgwBinFrame>,
        tx_result: &Sender<MsgTxResult>,
        detail_config: Arc<DetailConfig>,
    ) -> FrameResult<(u16, Arc<TdgwBinFrame>)> {
        //获取cl_ord_id 和 route_info
        let (cl_ord_id, mut route_info) = match self.tdgw_oms_2_gw_get_info_from_msg(&msg, detail_config) {
            Some((id, info)) => (id, info),
            None => {
                error!(
                    "msg_processor::NewTdgwOms2GwMsg::{:?},can't find route info or cl_ord_id from msg",
                    msg
                );
                return Err(ProtocolError::RouteNotFound);
            }
        };
        // 检查路由信息
        if !self.check_oms_2_gw_route(&mut route_info) {
            return Err(ProtocolError::RouteNotFound);
        }

        //路由处理
        if route_info.gw_id == constants::USERINFO_FIRST_BIT_VALID_VALUE {
            // 非指定网关
            let target_gw_id = self.generate_route_gw(cl_ord_id).ok_or(ProtocolError::TOE(1, ("route table is empty").to_string()))?;
            route_info.gw_id = self.conn_id_to_route_id_map.get(&target_gw_id).copied().ok_or(ProtocolError::RouteNotFound)?;
            
            let modified_msg = self.tdgw_gen_new_oms_2_gw_msg(msg, &route_info);

            match self.send_tdgw_msg(Arc::clone(&modified_msg), target_gw_id){
                core::result::Result::Ok(_) => core::result::Result::Ok((target_gw_id, Arc::clone(&modified_msg))),
                Err(err) => {
                    //1. send auto reject to oms
                    match auto_reject::tdgw_auto_reject(Arc::clone(&msg))
                    {
                        core::result::Result::Ok(auto_reject_msg) => {
                            if let Err(e) = self.send_tdgw_msg(Arc::clone(&auto_reject_msg), self.conn_id) {
                                msgProcessor_log_error!(self.conn_id, "tdgw_handle_oms_2_gw_msg, send auto reject to oms fail: {:?}",e);
                                
                                //send auto reject to oms fail, disconnect oms session
                                if let Err(e) = tx_result.send(MsgTxResult::Disconnect(self.conn_id)) {
                                    msgProcessor_log_error!(self.conn_id, "tdgw_handle_oms_2_gw_msg, send oms session Disconnect event fail: {:?}",e);
                                }
                            }
                            else
                            {
                                let now = self.start_time.elapsed().as_nanos();
                                if let Err(e) =
                                    self.msg_log_tx.send(MSGLOGENENT::OutShareOfferTdgwMsgInfo(
                                        now,
                                        self.conn_id,
                                        Arc::clone(&auto_reject_msg),
                                    ))
                                {
                                    error!(
                                        "send msg log error ,conid:{:?} error:{:?},close connection",
                                        self.conn_id, e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            msgProcessor_log_error!(self.conn_id,"tdgw_handle_oms_2_gw_msg, generate tdgw_auto_reject fail ,error:{:?}, msg:{:?}",e, msg);  
                        }
                    }
                    //2. tdgw_session statu change to exception,  remove from route_table , update route_table
                    //if let Err(e) = tx_result.send(MsgTxResult::GWSendFail(target_gw_id)) {
                    //    msgProcessor_log_error!(self.conn_id, "tdgw_handle_oms_2_gw_msg, send GWSendFail event fail: {:?}, target_gw_id={}", e, target_gw_id);
                    //}

                    core::result::Result::Err(err)
                }
            }
        } else {
            //  指定网关的msg理论上都会在主程序中直接送出，不会进入这个case
            if let Some(temp_gw_id) = self.verify_specified_gw(&route_info) {
                match self.send_tdgw_msg(Arc::clone(&msg), temp_gw_id as u16) {
                    core::result::Result::Ok(_) => core::result::Result::Ok((temp_gw_id, Arc::clone(&msg))),
                    Err(e) => core::result::Result::Err(e),
                }
            } else {
                error!(target: "msg_processor::NewTdgwOms2GwMsg::","msg_processor: {}, specified gw not in route table, routeinfo:{:?}", self.conn_id, route_info);
                Err(ProtocolError::RouteNotFound)
            }
        }
    }

    fn tgw_handle_oms_2_gw_msg(
        &mut self,
        msg: Arc<TgwBinFrame>,
        tx_result: &Sender<MsgTxResult>,
        detail_config: Arc<DetailConfig>,
    ) -> FrameResult<(u16, Arc<TgwBinFrame>)> {
        // get routinfo and cl_ord_id from msg
        let (cl_ord_id, mut route_info) = match self
            .tgw_oms_2_gw_get_info_from_msg(&msg, detail_config)
        {
            Some((id, info)) => (id, info),
            None => {
                error!(
                    "msg_processor::NewTgwOms2GwMsg::{:?},can't find route info or cl_ord_id from msg",
                    msg
                );
                return Err(ProtocolError::RouteNotFound);
            } // 继续处理下一条
        };
        //check routinfo
        if !self.check_oms_2_gw_route(&mut route_info) {
            if let Err(e) = tx_result.send(MsgTxResult::Disconnect(self.conn_id)) {
                error!(
                    "msg_processor::NewTgwOms2GwMsg:: {}, check route info, send Disconnect event fail: {:?},route_info={:?}",
                    self.conn_id, e, route_info
                );
                return Err(ProtocolError::RouteNotFound);
            }
        }

        //路由处理
        if route_info.gw_id == constants::USERINFO_FIRST_BIT_VALID_VALUE {
            // 非指定网关
            if let Some(target_gw_id) = self.generate_route_gw(cl_ord_id) {
                if let Some(target_gw_route_id) = self.conn_id_to_route_id_map.get(&target_gw_id) {
                    route_info.gw_id = *target_gw_route_id;
                } else {
                    return Err(ProtocolError::RouteNotFound);
                }
                // 修改userinfo

                let modified_msg = self.tgw_gen_new_oms_2_gw_msg(msg, &route_info);
                match self.send_tgw_msg(Arc::clone(&modified_msg), target_gw_id) {
                    core::result::Result::Ok(_) => {
                        core::result::Result::Ok((target_gw_id, modified_msg))
                    }
                    Err(e) => core::result::Result::Err(e),
                }
            } else {
                return Err(ProtocolError::RouteNotFound);
            }
        } else {
            // 指定网关
            if let Some(temp_gw_id) = self.verify_specified_gw(&route_info) {
                match self.send_tgw_msg(Arc::clone(&msg), temp_gw_id as u16) {
                    core::result::Result::Ok(_) => core::result::Result::Ok((temp_gw_id, msg)),
                    Err(e) => core::result::Result::Err(e),
                }
            } else {
                error!(target: "msg_processor::NewTgwOms2GwMsg::","msg_processor: {}, specified gw not in route table, routeinfo:{:?}", self.conn_id, route_info);
                Err(ProtocolError::RouteNotFound)
            }
        }
    }

    ///get routeInfo and cl_ord_id from tgw msg
    fn tgw_oms_2_gw_get_info_from_msg<'a>(
        &self,
        msg: &'a TgwBinFrame,
        session_detail: Arc<DetailConfig>,
    ) -> Option<(&'a [u8; 10], RouteInfo)> {
        match msg {
            TgwBinFrame::NewOrder100101New(new_order_100101) => Some((
                new_order_100101.get_cl_ord_id(),
                RouteInfo::new_from_tgw_user_info(
                    &new_order_100101.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ),
            )),
            TgwBinFrame::NewOrder100201New(new_order_100201) => Some((
                new_order_100201.get_cl_ord_id(),
                RouteInfo::new_from_tgw_user_info(
                    &new_order_100201.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ),
            )),
            TgwBinFrame::NewOrder104101New(new_order_104101) => Some((
                new_order_104101.get_cl_ord_id(),
                RouteInfo::new_from_tgw_user_info(
                    &new_order_104101.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ),
            )),
            TgwBinFrame::OrderCancelRequestNew(order_cancel_request) => Some((
                order_cancel_request.get_orig_cl_ord_id(),
                RouteInfo::new_from_tgw_user_info(
                    &order_cancel_request.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ),
            )),
            TgwBinFrame::SKip => {
                warn!(target: "business","msg_processor: {}, tgw_oms_2_gw_get_info_from_msg, skipp error msg:{:?}", self.conn_id,msg);
                return None;
            }
            _ => {
                warn!(target: "business","msg_processor: {}, tgw_oms_2_gw_get_info_from_msg, unsupport error msg:{:?}", self.conn_id,msg);
                return None;
            }
        }
    }

    fn tdgw_oms_2_gw_get_cl_ord_id<'a>(&self, msg: &'a TdgwBinFrame) -> Option<&'a [u8; 10]> {
        match msg {
            TdgwBinFrame::NewOrderSingleNew(new_order_single) => {
                Some(new_order_single.get_cl_ord_id())
            }
            TdgwBinFrame::OrderCancelRequestNew(order_cancel_request) => {
                Some(order_cancel_request.get_orig_cl_ord_id())
            }
            TdgwBinFrame::Skip => {
                warn!(target: "business","msg_processor: {}, tdgw_oms_2_gw_get_cl_ord_id skipp error msg:{:?}",self.conn_id,msg);
                None
            }
            _ => {
                warn!(target: "business","msg_processor: {}, tdgw_oms_2_gw_get_cl_ord_id unsupport error msg:{:?}",self.conn_id,msg);
                None
            }
        }
    }

    fn verify_specified_gw(&self, route_info: &RouteInfo) -> Option<u16> {
        if let Some(gw_route_id) = self.route_id_to_conn_id_map.get(&route_info.gw_id) {
            if self.routing_map.contains(&gw_route_id) {
                Some(*gw_route_id)
            } else {
                None
            }
        } else {
            None
        }
    }

    #[inline]
    fn tdgw_gen_new_oms_2_gw_msg(
        &self,
        msg: &Arc<TdgwBinFrame>,
        route_info: &RouteInfo,
    ) -> Arc<TdgwBinFrame> {
        let mut msg = Arc::clone(msg);
        let new_msg = Arc::make_mut(&mut msg);
        match new_msg {
            TdgwBinFrame::NewOrderSingleNew(new_order_single) => {
                new_order_single.set_user_info(route_info.get_tdgw_user_info());
                new_order_single.filled_head_and_tail();
                // TO DO 记录至redis
                //Self::record_2_redis(&msg);
            }
            TdgwBinFrame::OrderCancelRequestNew(order_cancel_request) => {
                order_cancel_request.set_user_info(route_info.get_tdgw_user_info());
                order_cancel_request.filled_head_and_tail();
                // TO DO 记录至redis
                //Self::record_2_redis(&msg);
            }
            TdgwBinFrame::Skip => {
                //warn!(target: "business","msg_processor: {}, tdgw_gw_2_oms_get_route skipp error msg:{:?},route_info:{:?}",self.conn_id,msg,route_info);
            }
            _ => {
                warn!(target: "business","msg_processor: {}, tdgw_gw_2_oms_get_route unsupport error msg:{:?},route_info:{:?}",self.conn_id,msg,route_info);
            }
        }
        msg
    }

    #[inline]
    fn tgw_gen_new_oms_2_gw_msg(
        &self,
        mut msg: Arc<TgwBinFrame>,
        route_info: &RouteInfo,
    ) -> Arc<TgwBinFrame> {
        let new_msg = Arc::make_mut(&mut msg);
        match new_msg {
            TgwBinFrame::NewOrder100101New(new_order_100101) => {
                new_order_100101.set_user_info(route_info.get_tgw_user_info());
                new_order_100101.filled_head_and_tail();
                // TO DO 记录至redis
                //Self::record_2_redis(&msg);
            },
            TgwBinFrame::NewOrder100201New(new_order_100201) => {
                new_order_100201.set_user_info(route_info.get_tgw_user_info());
                new_order_100201.filled_head_and_tail();
                // TO DO 记录至redis
                //Self::record_2_redis(&msg);
            },
            TgwBinFrame::NewOrder104101New(new_order_104101) => {
                new_order_104101.set_user_info(route_info.get_tgw_user_info());
                new_order_104101.filled_head_and_tail();
                // TO DO 记录至redis
                //Self::record_2_redis(&msg);
            }
            TgwBinFrame::OrderCancelRequestNew(order_cancel_request) => {
                order_cancel_request.set_user_info(route_info.get_tgw_user_info());
                order_cancel_request.filled_head_and_tail();
                // TO DO 记录至redis
                //Self::record_2_redis(&msg);
            }
            _ => {
                warn!(target: "business","msg_processor: {}, tgw_gen_new_oms_2_gw_msg unsupport error msg:{:?},route_info:{:?}",self.conn_id,msg,route_info);
            }
        }
        msg
    }

    //计算路由网关 tdgw/tgw 的clord_id位数一样
    #[inline]
    fn generate_route_gw(&self, cl_ord_id: &[u8; 10]) -> Option<u16> {
        if self.routing_map.len() == 0 {
            error!(target: "business","msg_processor: {}, generate_route_gw, routing_map is empty!",self.conn_id);
            return None;
        }

        // let mut hasher = DefaultHasher::new();
        // cl_ord_id.hash(&mut hasher);
        // let position = (hasher.finish() & 63) as usize;
        // self.routing_map.get(position).copied()
        // 取 cl_ord_id 的末尾字节作为数值（根据你的编码方式调整）
        //根据合同号后两位轮询
        let num = u16::from_be_bytes([cl_ord_id[8], cl_ord_id[9]]) as usize;
        let position = num % self.routing_map.len();
        self.routing_map.get(position).copied()
    }

    #[inline]
    pub fn tdgw_gw_2_oms_route_check(
        &self,
        msg: &TdgwBinFrame,
        route_info: &RouteInfo,
    ) -> Option<&u16> {
        if route_info.gw_id != 32 && route_info.oms_id != 32 && route_info.share_offer_id != 32 {
            self.route_id_to_conn_id_map.get(&route_info.oms_id)
        } else {
            None
        }
    }
    #[inline]
    pub fn tgw_gw_2_oms_route_check(&self, route_info: &RouteInfo) -> Option<&u16> {
        if route_info.gw_id != 32 && route_info.oms_id != 32 && route_info.share_offer_id != 32 {
            self.route_id_to_conn_id_map.get(&route_info.oms_id)
        } else {
            None
        }
    }

    // #[inline]
    // pub fn tdgw_gw_2_oms_get_route(&self, msg: &TdgwBinFrame, route_info: &RouteInfo) -> i32 {
    //     #[cfg(feature = "v_software")]
    //     match msg {
    //         TdgwBinFrame::OrderRejectNew(_) => {
    //             return (route_info.oms_id - 48) as i32;
    //         }
    //         TdgwBinFrame::CancelRejectNew(_) => return (route_info.oms_id - 48) as i32,
    //         TdgwBinFrame::ExecutionReportResponseNew(_) => {
    //             return (route_info.oms_id - 48) as i32;
    //         }
    //         TdgwBinFrame::ExecutionReportNew(_) => return (route_info.oms_id - 48) as i32,
    //         TdgwBinFrame::Skip => {
    //             warn!(target: "business","msg_processor: {}, tdgw_gw_2_oms_get_route skipp error msg:{:?},route_info:{:?}",self.conn_id,msg,route_info);
    //         }
    //         _ => {
    //             warn!(target: "business","msg_processor: {}, tdgw_gw_2_oms_get_route unsupport error msg:{:?},route_info:{:?}",self.conn_id,msg,route_info);
    //             return -1;
    //         }
    //     }
    //     -1
    // }

    // pub fn tgw_gw_2_oms_get_route(&self, msg: &TgwBinFrame, route_info: &RouteInfo) -> i32 {
    //     #[cfg(feature = "v_software")]
    //     //todo  BusinessRejectNew 待添加，需根据合同号从redis查询对应的oms来源
    //     match msg {
    //         TgwBinFrame::CancelRejectNew(_) => return (route_info.oms_id - 48) as i32,
    //         TgwBinFrame::ExecutionReportResponse200102New(_) => {
    //             return (route_info.oms_id - 48) as i32;
    //         }
    //         TgwBinFrame::ExecutionReport200115New(_) => return (route_info.oms_id - 48) as i32,
    //         TgwBinFrame::ExecutionReportResponse204102New(_) => {
    //             return (route_info.oms_id - 48) as i32;
    //         }
    //         TgwBinFrame::ExecutionReport204115New(_) => return (route_info.oms_id - 48) as i32,
    //         TgwBinFrame::SKip => {
    //             warn!(target: "business","msg_processor: {}, tdgw_gw_2_oms_get_route skipp error msg:{:?},route_info:{:?}",self.conn_id,msg,route_info);
    //         }
    //         _ => {
    //             warn!(target: "business","msg_processor: {}, tdgw_gw_2_oms_get_route unsupport error msg:{:?},route_info:{:?}",self.conn_id,msg,route_info);
    //             return -1;
    //         }
    //     }
    //     -1
    // }

    ///get routeInfo and cl_ord_id from tgw msg
    fn tdgw_oms_2_gw_get_info_from_msg<'a>(
        &self,
        msg: &'a TdgwBinFrame,
        session_detail: Arc<DetailConfig>,
    ) -> Option<(&'a [u8; 10], RouteInfo)> {
        match msg {
            TdgwBinFrame::NewOrderSingleNew(new_order_single)  => Some((
                new_order_single.get_cl_ord_id(),
                RouteInfo::new_from_tdgw_user_info(
                    &new_order_single.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ),
            )),
            TdgwBinFrame::OrderCancelRequestNew(order_cancel_request) => Some((
                order_cancel_request.get_orig_cl_ord_id(),
                RouteInfo::new_from_tdgw_user_info(
                    &order_cancel_request.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ),
            )),
            TdgwBinFrame::Skip => {
                warn!(target: "business","msg_processor: {}, tdgw_oms_2_gw_get_info_from_msg, skipp error msg:{:?}", self.conn_id,msg);
                return None;
            }
            _ => {
                warn!(target: "business","msg_processor: {}, tdgw_oms_2_gw_get_info_from_msg, unsupport error msg:{:?}", self.conn_id,msg);
                return None;
            }
        }
    }
    fn tdgw_generate_route_info_from_msg(
        &self,
        msg: &TdgwBinFrame,
        session_detail: Arc<DetailConfig>,
    ) -> Option<RouteInfo> {
        match msg {
            TdgwBinFrame::NewOrderSingleNew(new_order_single) => {
                Some(RouteInfo::new_from_tdgw_user_info(
                    &new_order_single.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TdgwBinFrame::OrderCancelRequestNew(order_cancel) => {
                Some(RouteInfo::new_from_tdgw_user_info(
                    &order_cancel.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TdgwBinFrame::OrderRejectNew(order_reject) => Some(RouteInfo::new_from_tdgw_user_info(
                &order_reject.get_user_info(),
                self.route_direction.clone(),
                self.route_link_type.clone(),
                Arc::clone(&session_detail),
            )),
            TdgwBinFrame::CancelRejectNew(cancel_reject) => {
                Some(RouteInfo::new_from_tdgw_user_info(
                    &cancel_reject.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TdgwBinFrame::ExecutionReportResponseNew(execution_report_response) => {
                Some(RouteInfo::new_from_tdgw_user_info(
                    &execution_report_response.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TdgwBinFrame::ExecutionReportNew(execution_report) => {
                Some(RouteInfo::new_from_tdgw_user_info(
                    &execution_report.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TdgwBinFrame::Skip => {
                warn!(target: "business","msg_processor: {}, tdgw_generate_route_info_from_msg, skipp error msg:{:?}", self.conn_id,msg);
                return None;
            }
            _ => {
                warn!(target: "business","msg_processor: {}, tdgw_generate_route_info_from_msg, unsupport error msg:{:?}", self.conn_id,msg);
                return None;
            }
        }
    }

    fn tgw_generate_route_info_from_msg(
        &self,
        msg: &TgwBinFrame,
        session_detail: Arc<DetailConfig>,
    ) -> Option<RouteInfo> {
        // todo  BusinessRejectNew待添加
        match msg {
            TgwBinFrame::NewOrder100101New(new_order_100101) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &new_order_100101.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            },
            TgwBinFrame::NewOrder100201New(new_order_100201) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &new_order_100201.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            },
            TgwBinFrame::NewOrder104101New(new_order_104101) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &new_order_104101.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TgwBinFrame::OrderCancelRequestNew(order_cancel) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &order_cancel.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TgwBinFrame::CancelRejectNew(cancel_reject) => Some(RouteInfo::new_from_tgw_user_info(
                &cancel_reject.get_user_info(),
                self.route_direction.clone(),
                self.route_link_type.clone(),
                Arc::clone(&session_detail),
            )),
            TgwBinFrame::ExecutionReportResponse200102New(execution_report_response_200102) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &execution_report_response_200102.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            },
            TgwBinFrame::ExecutionReportResponse200202New(execution_report_response_200202) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &execution_report_response_200202.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            },
            TgwBinFrame::ExecutionReportResponse204102New(execution_report_response_204102) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &execution_report_response_204102.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TgwBinFrame::ExecutionReport200115New(execution_report_200115) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &execution_report_200115.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            },
            TgwBinFrame::ExecutionReport200215New(execution_report_200215) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &execution_report_200215.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TgwBinFrame::ExecutionReport204115New(execution_report_204115) => {
                Some(RouteInfo::new_from_tgw_user_info(
                    &execution_report_204115.get_user_info(),
                    self.route_direction.clone(),
                    self.route_link_type.clone(),
                    Arc::clone(&session_detail),
                ))
            }
            TgwBinFrame::SKip => {
                warn!(target: "business","msg_processor: {}, tgw_generate_route_info_from_msg, skipp error msg:{:?}", self.conn_id,msg);
                return None;
            }
            _ => {
                warn!(target: "business","msg_processor: {}, tgw_generate_route_info_from_msg, unsupport error msg:{:?}", self.conn_id,msg);
                return None;
            }
        }
    }

    #[inline]
    fn check_oms_2_gw_route(&self, route_info: &mut RouteInfo) -> bool {
        // gw_id
        if route_info.gw_id < constants::USERINFO_FIRST_BIT_VALID_VALUE {
            error!(target: "business","msg_processor: {}, check_oms_2_gw_route fail: gw_id < {}, route_info={:?}",constants::USERINFO_FIRST_BIT_VALID_VALUE, self.conn_id, route_info);
            return false;
        }

        // oms_id
        let oms_check = (route_info.oms_id == self.route_id);
        if !oms_check {
            error!(target: "business","msg_processor: {}, check_oms_2_gw_route fail: oms_id error, route_info={:?}",self.conn_id, route_info);
            return false;
        }

        // share_offer_id
        if route_info.share_offer_id != TCPSHARECONFIG.share_offer_id {
            error!(target: "business","msg_processor: {}, check_oms_2_gw_route fail: share_offer_id error, route_info={:?}",self.conn_id, route_info);
            return false;
        }
        true
    }

    fn send_tdgw_msg(&self, frame: Arc<TdgwBinFrame>, conn_id: u16) -> FrameResult<()> {
        let tx_socket = self.mgr.find_conn_by_routing(conn_id as u16);
        match &*frame {
            TdgwBinFrame::NewOrderSingleNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TdgwBinFrame::OrderRejectNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TdgwBinFrame::OrderCancelRequestNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TdgwBinFrame::CancelRejectNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TdgwBinFrame::ExecutionReportNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TdgwBinFrame::ExecutionReportResponseNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TdgwBinFrame::Skip => {
                //warn!("skip msg not send:{:?}", msg)
                Err(ProtocolError::Skip)
            }
            _ => {
                // wrong message bo seend
                //warn!("unsupport msg not send:{:?}", frame)
                Err(ProtocolError::UnImplementedMethod)
            }
        }
    }

    fn send_tgw_msg(&self, frame: Arc<TgwBinFrame>, id: u16) -> FrameResult<()> {
        let tx_socket = self.mgr.find_conn_by_routing(id as u16);
        match &*frame {
            TgwBinFrame::NewOrder100101New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            },
            TgwBinFrame::NewOrder100201New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            },
            TgwBinFrame::NewOrder104101New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TgwBinFrame::OrderCancelRequestNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TgwBinFrame::CancelRejectNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TgwBinFrame::BusinessRejectNew(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TgwBinFrame::ExecutionReportResponse200102New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            },
            TgwBinFrame::ExecutionReportResponse200202New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            },
            TgwBinFrame::ExecutionReportResponse204102New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            },
            TgwBinFrame::ExecutionReport200115New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            },
            TgwBinFrame::ExecutionReport200215New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            TgwBinFrame::ExecutionReport204115New(msg) => {
                let data_bytes = msg.as_bytes_big_endian();
                tx_socket.tcp_conn_send_bytes(&data_bytes)
            }
            _ => {
                // wrong message bo seend
                //warn!("unsupport msg not send:{:?}", frame)
                Err(ProtocolError::UnImplementedMethod)
            }
        }
    }
}
