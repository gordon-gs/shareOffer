use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use fproto::stream_frame::tdgw_bin::TdgwBinFrame;
use fproto::stream_frame::tgw_bin::{TgwBinFrame, platform_info};
use share_offer::config::oms::{OMSCONFIG, OMSConfig};
use share_offer::config::{
    redis::REDISCONFIG, tcp_share::TCPSHARECONFIG, tdgw::TDGWCONFIG, tgw::TGWCONFIG,
};
use share_offer::constants;
use share_offer::log::{MSGLOGENENT, start_logging_thread};
use share_offer::msg_processor::{MsgProcessor, MsgRxEvent, MsgTxResult};
use share_offer::redis_client::{RedisWriteEvent, start_redis_write_thread};
use share_offer::session::{
    ConnType, DetailConfig, Session, SessionManager, SessionStatus, SessionType,
};
use share_offer_sys::tcp_connection;
use share_offer_sys::tcp_connection::{
    ConnectionProcessState, TCPConnection, tcp_conn_connect, tcp_conn_find_by_id, tcp_conn_listen,
};
use share_offer_sys::tcp_error::TCPLibcError;

use std::collections::HashMap;
use std::env;
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use tracing::{Level, debug, error, info, warn};
use tracing_subscriber::fmt::time;
use tracing_subscriber::{filter, prelude::*};

use chrono::Local;
use core_affinity;
use share_offer::route::{RouteDirection, RouteLinkType};

const NUM_BUSINESS_THREADS: usize = 3;

fn get_current_dir() -> PathBuf {
    env::current_dir().expect("无法获取当前工作目录")
}

fn init_log_config() -> Vec<tracing_appender::non_blocking::WorkerGuard> {
    let timer = time::OffsetTime::local_rfc_3339().expect("get local time");
    let mut layers = Vec::new();
    let mut guards = Vec::new();
    let file_appender_message = tracing_appender::rolling::hourly("log", "messages.log");
    let (non_blocking_message, _guard_message) =
        tracing_appender::non_blocking(file_appender_message);
    let layer_message = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_message)
        //.with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(filter::LevelFilter::INFO)
        .with_filter(filter::filter_fn(|metadata| {
            metadata.target().starts_with("messages")
        }))
        .boxed();

    layers.push(layer_message);
    guards.push(_guard_message);

    let file_appender_business = tracing_appender::rolling::hourly("log", "business.log");
    let (non_blocking_business, _guard_business) =
        tracing_appender::non_blocking(file_appender_business);
    let layer_business = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_business)
        //.with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(filter::LevelFilter::INFO)
        .with_filter(filter::filter_fn(|metadata| {
            metadata.target().starts_with("business")
        }))
        .boxed();
    layers.push(layer_business);
    guards.push(_guard_business);

    let file_appender_benchmark = tracing_appender::rolling::hourly("log", "benchmark.log");
    let (non_blocking_benchmark, _guard_benchmark) =
        tracing_appender::non_blocking(file_appender_benchmark);
    let layer_benchmark = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_benchmark)
        //.with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(filter::LevelFilter::INFO)
        .with_filter(filter::filter_fn(|metadata| {
            metadata.target().starts_with("benchmark")
        }))
        .boxed();
    layers.push(layer_benchmark);
    guards.push(_guard_benchmark);

    let file_appender_system = tracing_appender::rolling::hourly("log", "system.log");
    let (non_blocking_system, _guard_system) = tracing_appender::non_blocking(file_appender_system);
    let layer_system = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_system)
        //.with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(filter::LevelFilter::INFO)
        .with_filter(filter::filter_fn(|metadata| {
            metadata.target().starts_with("system")
        }))
        .boxed();
    layers.push(layer_system);
    guards.push(_guard_system);

    let (non_blocking_std, _guard_std) = tracing_appender::non_blocking(std::io::stdout());
    // #[cfg(debug_assertions)]
    // {
    let file_appender_debug = tracing_appender::rolling::hourly("log", "debug.log");
    let (non_blocking_debug, _guard_debug) = tracing_appender::non_blocking(file_appender_debug);
    let layer_debug = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_debug)
        //.with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(filter::LevelFilter::DEBUG)
        .with_filter(filter::filter_fn(|metadata| {
            !metadata.target().starts_with("messages")
                && !metadata.target().starts_with("business")
                && !metadata.target().starts_with("system")
                && !metadata.target().starts_with("benchmark")
        }))
        .boxed();
    layers.push(layer_debug);
    guards.push(_guard_debug);
    // }
    let file_appender_error = tracing_appender::rolling::never("log", "error.log");
    let (non_blocking_error, _guard_error) = tracing_appender::non_blocking(file_appender_error);
    let layer_warn = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_error)
        //.with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(filter::LevelFilter::ERROR)
        .and_then(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking_std)
                .with_timer(timer.clone())
                .with_filter(filter::LevelFilter::INFO)
                .with_filter(filter::filter_fn(|metadata| {
                    *metadata.level() != Level::ERROR
                })),
        )
        .with_filter(filter::filter_fn(|metadata| {
            !metadata.target().starts_with("messages")
                && !metadata.target().starts_with("business")
                && !metadata.target().starts_with("system")
                && !metadata.target().starts_with("benchmark")
        }))
        .boxed();
    layers.push(layer_warn);
    guards.push(_guard_error);
    tracing_subscriber::registry().with(layers).init();
    guards
}

fn main() {
    let current_path = get_current_dir();
    let _guards = init_log_config();
    println!(
        "share offer begin to run,work dir:{}",
        current_path.to_string_lossy()
    );
    info!(target: "system",  "share offer begin to run,work dir:{}",
        current_path.to_string_lossy());

    let mut app = ShareOffer::new();
    app.init();
    app.run();
}

struct ShareOffer {
    pipe_epoll_fd: tcp_connection::TCPEventEpoll,
    business_channels: HashMap<u16, Sender<MsgRxEvent>>,
    result_channels: (Sender<MsgTxResult>, Receiver<MsgTxResult>),
    routing_map: HashMap<u16, Vec<u16>>, //Relative conn id
    g_mgr: Arc<tcp_connection::TCPConnectionManager>,
    session_manager: SessionManager,
    start_time: Instant,
    msg_log_tx: Sender<MSGLOGENENT>,
    msg_log_rx: Receiver<MSGLOGENENT>,
    redis_write_tx: Option<Sender<RedisWriteEvent>>,
}

impl ShareOffer {
    fn new() -> Self {
        // 步骤 1: 创建结果通道（业务线程 → 接收线程）
        let result_channels = unbounded();

        let config_file = CString::new(TCPSHARECONFIG.config_file_path.clone()).unwrap();

        let mgr = unsafe { tcp_connection::tcp_conn_mgr_create(config_file.as_ptr()) };

        info!(target: "system",  "tcp_conn_mgr_create returned: {:p}",mgr);

        if mgr.is_null() {
            error!(target: "system","Failed to create TCP connection manager with config file:{}", TCPSHARECONFIG.config_file_path);
            std::process::exit(1);
        } else {
            info!(target: "system",  "Successfully created TCP connection manager");
            info!(target: "system",  "tcp_share_config:{:?}",*TCPSHARECONFIG);
        }

        info!(target: "system",  "create log channel begin");
        let (msg_log_tx, msg_log_rx) = unbounded();
        info!(target: "system",  "create log channel success ");

        let local_now = Local::now();
        let start_instance: Instant = Instant::now();
        info!(target: "system","开始基准时间: {},instance:{}", local_now.format("%Y-%m-%d %H:%M:%S%.9f"),start_instance.elapsed().as_nanos());

        Self {
            pipe_epoll_fd: tcp_connection::TCPEventEpoll::new(),
            business_channels: HashMap::new(),
            result_channels,
            routing_map: HashMap::new(),
            g_mgr: Arc::new(tcp_connection::TCPConnectionManager { mgr }),
            session_manager: Default::default(),
            start_time: Instant::now(),
            msg_log_tx,
            msg_log_rx,
            redis_write_tx: None,
        }
    }

    fn init(&mut self) {
        /*self.session_manager
        .init_redis_from_config(REDISCONFIG.cluster_nodes.clone())
        .unwrap_or_else(|e| {
            error!(target: "system", "Redis initialization failed: {}", e);
            panic!("Redis is required but initialization failed: {}", e);
        });*/
        if cfg!(feature = "no_redis") {
            info!(target: "system", "not Initializing Redis...");
        } else {
            info!(target: "system", "Initializing Redis...");
            if let Err(e) = self
                .session_manager
                .init_redis_from_config(REDISCONFIG.cluster_nodes.clone())
            {
                error!(target: "system", "Redis initialization failed: {}", e);
                error!(target: "system", "Redis is required for message forwarding. Exiting.");
                std::process::exit(1);
            }
            self.init_redis_write_thread();
            info!(target: "system", "success init Redis write thread");
        }
        self.init_msg_log_thread();
        info!(target: "system",  "success init log thread");
        self.init_msg_processors();
        info!(target: "system",  "success init msg_processor");
        self.init_sessions_config();
        info!(target: "system", "Sessions config initialized");
        if !cfg!(feature = "no_redis") {
            if let Some(tx) = &self.redis_write_tx {
                for event in self.session_manager.build_gw_list_events() {
                    let _ = tx.send(event);
                }
            }
            info!(target: "system", "Initializing ID mapping...");
            if let Err(e) = self.session_manager.init_id_mapping() {
                error!(target: "system", "ID mapping initialization failed: {}", e);
                error!(target: "system", "ID mapping is required for message forwarding. Exiting.");
                std::process::exit(1);
            }
            info!(target: "system", "ID mapping initialized");
        }

        self.init_connections();
        info!(target: "system",  "success init connection");

        //unsafe { tcp_connection::tcp_conn_mgr_start(self.g_mgr) }
    }

    fn init_msg_log_thread(&mut self) {
        _ = start_logging_thread(self.msg_log_rx.clone());
    }

    fn init_redis_write_thread(&mut self) {
        let (tx, rx) = unbounded::<RedisWriteEvent>();
        let nodes = REDISCONFIG.cluster_nodes.clone();
        let _ = start_redis_write_thread(rx, nodes);
        self.redis_write_tx = Some(tx);
    }

    fn init_msg_processors(&mut self) {
        // 根据配置文件，为每个conn创建业务线程的输入通道（分发线程 → 业务线程）
        // 每个业务线程有独立的 Sender/Receiver
        let mut business_channels = HashMap::new();
        for (conn_id, conn_type, (tx, rx)) in TCPSHARECONFIG
            .connections
            .iter()
            .map(|c| (c.conn_id, c.conn_type.clone(), unbounded()))
        {
            let route_direction = match conn_type.as_str() {
                "server" => RouteDirection::OMS2GW,
                "client" => RouteDirection::GW2OMS,
                _ => {
                    panic!("init_msg_processors, wrong conn_type type {}", conn_type);
                }
            };
            let route_link_type = if cfg!(feature = "v_software") {
                RouteLinkType::Software
            } else {
                RouteLinkType::Hardware
            };
            business_channels.insert(conn_id, tx);
            let tx_result_clone = self.result_channels.0.clone();
            let now = self.start_time.clone();
            // 启动线程（移动捕获 tx 和 tx_result_clone）
            let core_ids = core_affinity::get_core_ids().unwrap();

            let bind_core = if core_ids.len() > ((conn_id as usize + 1) * 2) {
                core_ids[(conn_id as usize + 1) * 2]
            } else {
                core_ids[0]
            };
            let mgr_arc = Arc::clone(&self.g_mgr);
            let msg_log_tx = self.msg_log_tx.clone();
            let conn_id_to_route_id_map = TCPSHARECONFIG.conn_id_to_route_id_map.clone();
            let route_id_to_conn_id_map = TCPSHARECONFIG.route_id_to_conn_id_map.clone();
            let route_id: u16;
            if let Some(temp_route_id) = conn_id_to_route_id_map.get(&conn_id) {
                route_id = *temp_route_id
            } else {
                panic!(
                    "init_msg_processors, wrong conn_id,conn_route_id not config! conn_id={:?}",
                    conn_id
                );
            }
            thread::spawn(move || {
                if !cfg!(feature = "local_debug") {
                    if bind_core.id != 0 {
                        core_affinity::set_for_current(bind_core);
                    }
                }

                // 业务线程 ID（通过索引生成唯一 ID）
                let mut msg_processor = MsgProcessor {
                    routing_map: Vec::new(),
                    conn_id: conn_id.clone(),
                    route_id: route_id,
                    route_direction,
                    route_link_type,
                    start_time: now,
                    mgr: mgr_arc,
                    msg_log_tx: msg_log_tx,
                    conn_id_to_route_id_map,
                    route_id_to_conn_id_map,
                };
                msg_processor.business_thread(rx, tx_result_clone);
            });
            info!(target: "system",  "business thread: {}, conn_type:{:?} started", conn_id , conn_type);
        }
        self.business_channels = business_channels;
    }

    fn init_sessions_config(&mut self) {
        for connection in &TCPSHARECONFIG.connections {
            let session = match connection.conn_type.as_str() {
                "server" => {
                    //预留位置检查TOE的配置端口是否配置OMS业务信息
                    let oms_config = OMSCONFIG
                        .server_id_to_session_map
                        .get(&connection.conn_tag)
                        .expect("wrong oms config");
                    Session {
                        conn_id: connection.conn_id,
                        route_id: connection.route_id,
                        conn_tag: connection.conn_tag.clone(),
                        local_connect_str: format!(
                            "{}:{}",
                            connection.local_ip, connection.local_port
                        ),
                        remote_connect_str: format!(
                            "{}:{}",
                            connection.remote_ip, connection.remote_port
                        ),
                        remote_id: oms_config.server_id.clone(),
                        last_read_time_ms: 0,
                        last_write_time_ms: 0,
                        heart_beat: OMSCONFIG.heart_bt_int,
                        time_out_count: 0,
                        reconnect_interval: OMSCONFIG.reconnect_interval,
                        status: Default::default(),
                        session_type: SessionType::OMS,
                        conn_type: ConnType::SERVER,
                        conn: tcp_connection::get_tcp_connection(
                            self.g_mgr.mgr,
                            connection.conn_id,
                        ),
                        detail_config: DetailConfig::OMSINFO(oms_config.clone()).into(),
                    }
                }
                #[cfg(feature = "tgw")]
                "client" => {
                    let tgw_config = TGWCONFIG
                        .session_id_to_session_map
                        .get(&connection.conn_tag)
                        .expect("wrong tgw config");
                    Session {
                        conn_id: connection.conn_id,
                        route_id: connection.route_id,
                        conn_tag: connection.conn_tag.clone(),
                        local_connect_str: "".to_string(),
                        remote_connect_str: format!(
                            "{}:{}",
                            connection.remote_ip, connection.remote_port
                        ),
                        remote_id: tgw_config.target_comp_id.clone(),
                        last_read_time_ms: 0,
                        last_write_time_ms: 0,
                        heart_beat: TGWCONFIG.heart_bt_int,
                        time_out_count: 0,
                        reconnect_interval: TGWCONFIG.reconnect_interval,
                        status: Default::default(),
                        conn_type: ConnType::CLIENT,
                        session_type: SessionType::TGW,
                        conn: tcp_connection::get_tcp_connection(
                            self.g_mgr.mgr,
                            connection.conn_id,
                        ),
                        detail_config: DetailConfig::TGWINFO(tgw_config.clone()).into(),
                    }
                }
                #[cfg(feature = "tdgw")]
                "client" => {
                    let tdgw_config = TDGWCONFIG
                        .session_id_to_session_map
                        .get(&connection.conn_tag)
                        .expect("wrong tdgw config");
                    Session {
                        conn_id: connection.conn_id,
                        route_id: connection.route_id,
                        conn_tag: connection.conn_tag.clone(),
                        local_connect_str: "".to_string(),
                        remote_connect_str: format!(
                            "{}:{}",
                            connection.remote_ip, connection.remote_port
                        ),
                        remote_id: tdgw_config.target_comp_id.clone(),
                        last_read_time_ms: 0,
                        last_write_time_ms: 0,
                        heart_beat: TDGWCONFIG.heart_bt_int,
                        time_out_count: 0,
                        reconnect_interval: TDGWCONFIG.reconnect_interval,
                        status: Default::default(),
                        conn_type: ConnType::CLIENT,
                        session_type: SessionType::TDGW,
                        conn: tcp_connection::get_tcp_connection(
                            self.g_mgr.mgr,
                            connection.conn_id,
                        ),
                        detail_config: DetailConfig::TDGWINFO(tdgw_config.clone()).into(),
                    }
                }
                _ => {
                    panic!(
                        "tcp_share_config.json wrong connection type {}",
                        connection.conn_type
                    );
                }
            };
            self.session_manager.add_session(session);
        }
    }

    //1. connect gw_session, add to reconnect list if connect failed; 2.add oms_session to reconnect list
    fn init_connections(&mut self) {
        unsafe {
            for connection in &TCPSHARECONFIG.connections {
                match self
                    .session_manager
                    .get_session_by_conn_id(connection.conn_id)
                {
                    Some(session) => {
                        if session.conn_type == ConnType::CLIENT {
                            let conn = tcp_conn_find_by_id(self.g_mgr.mgr, connection.conn_id);
                            let ret = tcp_conn_connect(conn);
                            info!(target: "system",  "init connections, connect ret:{}, conn_id={:?},route_id={}, conn_tag={:?}", ret, session.conn_id,TCPSHARECONFIG.route_id_to_conn_id_map.get(&session.conn_id).unwrap_or(&999), session.conn_tag);
                            if ret != 0 {
                                self.session_manager
                                    .add_session_to_reconnect(session.conn_id.clone());
                            }
                            self.pipe_epoll_fd
                                .add_connection(&TCPConnection { conn })
                                .unwrap();
                        } else {
                            self.session_manager
                                .add_session_to_reconnect(session.conn_id.clone());
                        }
                    }
                    None => {}
                }
            }
        }
    }

    fn run(&mut self) {
        let mut last_process_time = self.start_time.elapsed().as_nanos();
        loop {
            // batch process rx events
            let rx_events = self.pipe_epoll_fd.get_ready_events();
            let now: u128 = self.start_time.elapsed().as_nanos();
            for mut rx_event in rx_events {
                let rx_pipe_fd = rx_event.get_event_fd();
                let conn_event = tcp_connection::read_fd_event(rx_pipe_fd);
                match conn_event.type_ as tcp_connection::conn_event_type_t {
                    tcp_connection::conn_event_type_t_TCP_EVENT_RX_READY => {
                        // todo!(use correct conn_type)
                        match self
                            .session_manager
                            .get_session_by_conn_id(conn_event.conn_id as u16)
                        {
                            Some(session) => {
                                let origin_session = session.clone();
                                if session.session_type == SessionType::OMS {
                                    #[cfg(feature = "tgw")]
                                    self.handle_oms_2_tgw_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Normal,
                                    );
                                    #[cfg(feature = "tdgw")]
                                    self.handle_oms_2_tdgw_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Normal,
                                    );
                                    //self.pipe_epoll_fd.send_wakeup();
                                } else {
                                    // feature tgw
                                    #[cfg(feature = "tgw")]
                                    self.handle_tgw_2_oms_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Normal,
                                    );
                                    // feature tdgw
                                    #[cfg(feature = "tdgw")]
                                    self.handle_tdgw_2_oms_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Normal,
                                    );
                                }
                                //update session read time
                                self.session_manager
                                    .update_read_time_by_conn_id(now, conn_event.conn_id);
                                //self.pipe_epoll_fd.send_wakeup();
                            }
                            None => {
                                // check
                                error!("seesion id={:?} not found!", conn_event.conn_id as u16)
                            }
                        }
                    }
                    tcp_connection::conn_event_type_t_TCP_EVENT_CONNECTED => {
                        info!(target:"system","conn_status_change::conn_id={:?},route_id={:?},status=tcp_connection=TCP_EVENT_CONNECTED",conn_event.conn_id as u16,TCPSHARECONFIG.conn_id_to_route_id_map.get(&conn_event.conn_id).unwrap_or(&999));

                        //new accept or connected,
                        //(1)set session state = connected (2) logon gw
                        self.session_manager
                            .process_session_connected_event(now, conn_event.conn_id);
                    }
                    tcp_connection::conn_event_type_t_TCP_EVENT_CLOSED => {
                        info!(target:"system","conn_status_change::conn_id={:?},route_id={:?},status=tcp_connection=TCP_EVENT_CLOSED",conn_event.conn_id as u16,TCPSHARECONFIG.conn_id_to_route_id_map.get(&conn_event.conn_id).unwrap_or(&999));
                        let is_need_ops = self
                            .session_manager
                            .process_tcp_conn_closed_event(now, conn_event.conn_id);
                        if !cfg!(feature = "no_redis") {
                            let disconnect_events = self
                                .session_manager
                                .build_gw_disconnect_events(conn_event.conn_id);
                            // manage memory, update self.routing_map
                            if let Some(tx) = &self.redis_write_tx {
                                for ev in disconnect_events {
                                    let _ = tx.send(ev);
                                }
                            }
                        }

                        if let Some(session) = self
                            .session_manager
                            .get_session_by_conn_id(conn_event.conn_id)
                        {
                            match &session.session_type {
                                SessionType::OMS => {
                                    self.session_manager.on_oms_disconnect(conn_event.conn_id);
                                    //todo 断开是否还有其他操作
                                    //TGW或TDGW重连
                                }
                                //TODO process_tcp_conn_closed_event？
                                SessionType::TDGW => {
                                    if is_need_ops {
                                        self.update_gw_routing_table();
                                        self.session_manager.gw_begin_reconnect(conn_event.conn_id);
                                    }
                                }
                                SessionType::TGW => {
                                    if is_need_ops {
                                        self.update_gw_routing_table();
                                        self.session_manager.gw_begin_reconnect(conn_event.conn_id);
                                    }
                                }
                            }
                        }
                    }
                    tcp_connection::conn_event_type_t_TCP_EVENT_ERROR => {
                        info!(target:"system","conn_status_change::conn_id={:?},route_id={:?},status=tcp_connection=TCP_EVENT_ERROR",conn_event.conn_id as u16,TCPSHARECONFIG.conn_id_to_route_id_map.get(&conn_event.conn_id).unwrap_or(&999));
                        //get errorcode from conn_event.resv, error handle
                        match self
                            .session_manager
                            .get_session_by_conn_id(conn_event.conn_id as u16)
                        {
                            Some(session) => {
                                let origin_session = session.clone();
                                if session.session_type == SessionType::OMS {
                                    error!(
                                        "seesion id:{:?} tcp error:{}",
                                        conn_event.conn_id as u16,
                                        TCPLibcError::from(conn_event.error_code)
                                    );
                                    // disconnect oms session
                                    if session.status != SessionStatus::Closing && session.status != SessionStatus::Disconnected{
                                        self.session_manager
                                            .set_session_status_by_conn_id(
                                                conn_event.conn_id as u16,
                                                SessionStatus::WaitDisconnect,
                                        ); 
                                    } 
                                    
                                    #[cfg(feature = "tgw")]
                                    self.handle_oms_2_tgw_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Error,
                                    );
                                    #[cfg(feature = "tdgw")]
                                    self.handle_oms_2_tdgw_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Error,
                                    );
                                    //self.pipe_epoll_fd.send_wakeup();
                                } else {
                                    //move error gw out from routing table , and reconnect gw session
                                    if session.status == SessionStatus::Ready{
                                        self.session_manager
                                            .set_session_status_by_conn_id(
                                                conn_event.conn_id as u16,
                                                SessionStatus::WaitDisconnect,
                                        );
                                        self.update_gw_routing_table(); 
                                    }
                                    // feature tgw
                                    #[cfg(feature = "tgw")]
                                    self.handle_tgw_2_oms_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Error,
                                    );
                                    // feature tdgw
                                    #[cfg(feature = "tdgw")]
                                    self.handle_tdgw_2_oms_msg(
                                        &mut rx_event,
                                        now,
                                        &conn_event,
                                        &origin_session,
                                        &ConnectionProcessState::Error,
                                    );
                                }
                            }
                            None => {
                                // check
                                error!("seesion id:{:?} not found!", conn_event.conn_id as u16)
                            }
                        }
                        /*self.session_manager.process_tcp_conn_error_event(
                            now,
                            conn_event.conn_id,
                            conn_event.resv,
                        );*/
                    }
                    tcp_connection::conn_event_type_t_TCP_EVENT_NONE => {
                        // initial statu, skip
                        info!(target:"system","conn_status_change::conn_id={:?},route_id={:?},status=tcp_connection=conn_event_type_t_TCP_EVENT_NONE",conn_event.conn_id as u16,TCPSHARECONFIG.conn_id_to_route_id_map.get(&conn_event.conn_id).unwrap_or(&999));
                    }
                    tcp_connection::conn_event_type_t_TCP_EVENT_TX_READY => {
                        // ignore, won't get this event,it only writes to tx_buf.pipe_fd[1]
                        info!(target:"system","conn_status_change::conn_id={:?},route_id={:?},status=TCP_EVENT_TX_READY",conn_event.conn_id as u16,TCPSHARECONFIG.conn_id_to_route_id_map.get(&conn_event.conn_id).unwrap_or(&999));
                    }
                    tcp_connection::conn_event_type_t_TCP_EVENT_CLOSING => {
                        //tcp_status before closed, happens when:
                        //disable_connection or close_connection or force_close_connection
                        info!(target:"system","conn_status_change::conn_id={:?},route_id={:?},status=tcp_connection=TCP_EVENT_CLOSING",conn_event.conn_id as u16,TCPSHARECONFIG.conn_id_to_route_id_map.get(&conn_event.conn_id).unwrap_or(&999));

                        self.session_manager
                            .process_tcp_conn_closing_event(now, conn_event.conn_id);
                    }
                    _ => {
                        // log: conn_id, conn_event.type, conn_event.resv
                        info!(target:"system","conn_status_change::conn_id={:?},route_id={:?},status=tcp_connection=UnKnown:{:?}",conn_event.conn_id as u16,TCPSHARECONFIG.conn_id_to_route_id_map.get(&conn_event.conn_id).unwrap_or(&999),conn_event.type_);
                    }
                }
                // debug!("end to socket event count: {}", event_count);
            }

            loop {
                //self.pipe_epoll_fd.send_wakeup();
                match self.result_channels.1.try_recv() {
                    Ok(MsgTxResult::NewTgwMsg(_, id)) => {
                        // zero copy write to target tx buf
                        // todo!(new enum business logic only or add match)
                        break;
                    }
                    Ok(MsgTxResult::NewTdgwMsg(_, id)) => {
                        self.session_manager
                            .update_write_time_by_conn_id(now, id as u16);
                        break;
                    }
                    Ok(MsgTxResult::Disconnect(id)) => {
                        self.session_manager
                            .set_session_status_by_conn_id(id, SessionStatus::WaitDisconnect);
                        error!(target:"system","tx result, disconnect event, conn_id={:?}",id);
                    }
                    Err(TryRecvError::Empty) => {
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        // todo!(deal exception)
                        break;
                    }
                }
            }
            if (now - last_process_time) / 10000000 > 5 {
                //info!(target: "system",  "update session manager,time:{}",now);
                self.session_manager.process_heart_beats_event(now);

                if self.session_manager.process_wait_disconnect_event(now) {
                    self.update_gw_routing_table();
                }
                self.session_manager.process_session_reconnect_event(
                    now,
                    self.g_mgr.mgr,
                    &mut self.pipe_epoll_fd,
                );
                last_process_time = now
            }
        }
    }

    /// 计算消息的目标业务线程索引（哈希轮询算法）
    /// msg: 待分发的消息（i32）
    /// num_threads: 业务线程总数
    /// 返回: 目标线程的索引（0..num_threads-1）
    fn hash_round_robin(msg: &TdgwBinFrame, num_threads: usize) -> usize {
        // let hash = Self::compute_hash(msg);

        // // 哈希值对线程数取模，得到目标索引（确保均匀分布）
        // hash % num_threads
        1
    }

    fn compute_hash(msg: &TdgwBinFrame) -> usize {
        todo!()
    }

    //生成网关路由表
    fn update_gw_routing_table(&mut self) {
        let mut ready_conn_id_map = self.session_manager.get_ready_gw_conn_ids();
        debug!(
            "generate routing table,ready_conn_id ={:?}",
            ready_conn_id_map
        );
        ready_conn_id_map
            .iter_mut()
            .for_each(|(_, ready_conn_ids)| {
                // 循环重复数组并截取前64个元素
                let new_ids: Vec<u16> = ready_conn_ids.iter().cycle().take(64).cloned().collect();
                *ready_conn_ids = new_ids;
            });
        self.routing_map = ready_conn_id_map;

        for (conn_id, tx) in &self.business_channels {
            // 给所有的业务线程更新routing_map
            match self.session_manager.get_session_by_conn_id(*conn_id) {
                None => {
                    continue;
                }
                Some(session) => {
                    let platform_id = match **&session.detail_config {
                        DetailConfig::TDGWINFO(ref tdgw_detail) => tdgw_detail.platform_id,
                        DetailConfig::TGWINFO(ref tgw_detail) => tgw_detail.platform_id,
                        DetailConfig::OMSINFO(ref oms_detail) => oms_detail.platform_id,
                        _ => {
                            info!(target:"system","update_gw_routing_table cannot find detail config,connd_id:{:?},config:{:?}",conn_id,session.detail_config);
                            continue;
                        }
                    };
                    match tx.send(MsgRxEvent::UpdateMap(
                        self.routing_map.entry(platform_id).or_default().clone(),
                    )) {
                        Ok(_) => {
                            info!(target: "system","MsgRxEvent::UpdateMap success ,conn_id={:?}",conn_id)
                        }
                        Err(err) => {
                            info!(target: "system","MsgRxEvent::UpdateMap fail :{:?}",err);
                        }
                    }
                }
            }
        }
        info!(target: "system","cur routing_map:{:?}",self.routing_map);
    }

    fn handle_oms_2_tdgw_msg(
        &mut self,
        rx_event: &mut TCPConnection,
        now: u128,
        conn_event: &tcp_connection::tcp_conn_event_t,
        origin_session: &Session,
        connectionState: &ConnectionProcessState,
    ) {
        loop {
            match rx_event.parse_frame::<TdgwBinFrame>(connectionState) {
                Ok(Some(frame)) => {
                    // process upload stream
                    match &*frame {
                        TdgwBinFrame::LogonNew(logon) => {
                            // reply platform state + platform info(redis or simulate tgw?) + update self.routing_map
                            // reply logon

                            //info!(target: "messages::oms::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,logon);
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid={:?} error={:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            if self.session_manager.process_oms_logon_msg_tdgw(
                                now,
                                conn_event.conn_id,
                                &logon,
                            ) {
                                //send platform state
                                let so_state = self.session_manager.get_so_platform_status();
                                if let Err(e) = self.session_manager.send_so_platform_status_to_oms(
                                    conn_event.conn_id,
                                    now,
                                    so_state,
                                ) {
                                    error!(
                                        "send so platrom status,conid={:?} error={:?},close connection",
                                        conn_event.conn_id, e
                                    );
                                    self.session_manager.set_session_status_by_conn_id(
                                        conn_event.conn_id,
                                        SessionStatus::WaitDisconnect,
                                    );
                                    continue;
                                }
                                //todo: send exec_rpt_info
                                if let Err(e) = self
                                    .session_manager
                                    .tdgw_send_execrptinfo_for_test(now, conn_event.conn_id)
                                {
                                    error!(
                                        "tdgw_send_execrptinfo_for_test error:{:?},close connection",
                                        e
                                    );
                                    continue;
                                }

                                self.session_manager.set_session_status_by_conn_id(
                                    conn_event.conn_id,
                                    SessionStatus::Ready,
                                );
                            }
                        }
                        TdgwBinFrame::LogoutNew(logout) => {
                            // info!(
                            //     "messages::oms::in, conn_id={:?},time={:?},msg={:?}",
                            //     conn_event.conn_id, now, logout
                            // );
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid={:?} error={:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            self.session_manager.set_session_status_by_conn_id(
                                conn_event.conn_id,
                                SessionStatus::WaitDisconnect,
                            );
                        }
                        TdgwBinFrame::HeartbeatNew(heartbeat) => {
                            // process flash server hb ,log
                            //info!(target: "messages::oms::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,heartbeat);
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid={:?} error={:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                        }
                        TdgwBinFrame::ExecRptSyncNew(exec_rpt_sync) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conn_id={:?} error={:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            if cfg!(feature = "no_redis") {
                                for sync_request in exec_rpt_sync.get_no_groups_3() {
                                    let pbu = sync_request.get_pbu();
                                    let pbu_str = String::from_utf8_lossy(pbu).trim().to_string();
                                    let set_id = sync_request.get_set_id();
                                    let begin_index = sync_request.get_begin_report_index();
                                    let latest_index = match self
                                        .session_manager
                                        .get_latest_report_index(&pbu_str, set_id)
                                    {
                                        Ok(index) => index,
                                        Err(e) => {
                                            error!(target: "system", "Failed to get latest report index: pbu={}, set_id={}, error={}",
                                                                           pbu_str, set_id, e);
                                            continue;
                                        }
                                    };
                                    if latest_index < begin_index {
                                        continue;
                                    }

                                    match self.session_manager.batch_get_execution_reports(
                                        &pbu_str,
                                        set_id,
                                        begin_index,
                                        latest_index,
                                    ) {
                                        Ok(reports) => {
                                            info!(target: "business", "Report sync: fetched {} reports for pbu={}, set_id={}, range={}..{}",
                                                                          reports.len(), pbu_str, set_id, begin_index, latest_index);

                                            if let Some(session) = self
                                                .session_manager
                                                .get_session_by_conn_id(conn_event.conn_id)
                                            {
                                                for (report_index, report_data) in reports {
                                                    if let Err(e) = session
                                                        .conn
                                                        .tcp_conn_send_bytes(&report_data)
                                                    {
                                                        error!(target: "system", "Failed to send report to OMS: pbu={}, set_id={}, index={}, error={:?}",
                                                                                       pbu_str, set_id, report_index, e);
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!(target: "system", "Failed to fetch reports from Redis: pbu={}, set_id={}, error={}",
                                                                           pbu_str, set_id, e);
                                        }
                                    }
                                }
                            }
                            // 重拉回报 Java 柜台直接读 Redis
                            //info!(target: "messages::oms::in", "conn_id={:?}, time={:?}, ExecRptSyncNew ignored (Java reads Redis directly)", conn_event.conn_id, now);
                            self.session_manager
                                .update_read_time_by_conn_id(now, conn_event.conn_id);
                        }
                        TdgwBinFrame::NewOrderSingleNew(new_order_single) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,connid={:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            let bench_time = self.start_time.elapsed().as_nanos();
                            if let Err(e) =
                                self.msg_log_tx.send(MSGLOGENENT::BenchmarkInOMSTdgwInfo(
                                    1,
                                    bench_time,
                                    bench_time - now,
                                    conn_event.conn_id,
                                    Arc::clone(&frame),
                                ))
                            {
                                error!(
                                    "send msg log error ,conn_id={:?} error={:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            //info!(target: "messages::oms::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,new_order_single);
                            //info!(target: "benchmark::oms::in::recv", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                            if (new_order_single.user_info[0] as u16) < 48
                                || (new_order_single.user_info[0] as u16) > 127
                            {
                                error!(
                                    "oms in newOrder  userinfo error! conn_id={:?},msg:{},user_info:{:?}",
                                    conn_event.conn_id,
                                    new_order_single,
                                    new_order_single.user_info
                                );
                                continue;
                            }
                            if new_order_single.user_info[0] as u16
                                != constants::USERINFO_FIRST_BIT_VALID_VALUE
                            {
                                //info!(target:"business::mark","new order in quick,userinfo{:?}",new_order_single.user_info);
                                let id: u16;
                                if let Some(target_conn_id) = TCPSHARECONFIG
                                    .route_id_to_conn_id_map
                                    .get(&(new_order_single.user_info[0] as u16))
                                {
                                    id = *target_conn_id
                                } else {
                                    error!(
                                        "oms in newOrder  userinfo error! conn_id={:?},msg:{},user_info:{:?}",
                                        conn_event.conn_id,
                                        new_order_single,
                                        new_order_single.user_info
                                    );
                                    continue;
                                }
                                self.session_manager.record_order(
                                    new_order_single.get_cl_ord_id().clone(),
                                    conn_event.conn_id,
                                );
                                let send_data = new_order_single.as_bytes_big_endian();
                                //info!(target: "benchmark::oms::in::ready_to_send", "target_conn_id={:?},time={:?},msg={:?}",id,self.start_time.elapsed().as_nanos(),new_order_single);
                                match self
                                    .g_mgr
                                    .find_conn_by_routing(id)
                                    .tcp_conn_send_bytes(&send_data)
                                {
                                    Ok(_) => {
                                        //info!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={:?}",id,now,new_order_single);
                                        if let Err(e) =
                                            self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgdwInfo(
                                                1,
                                                self.start_time.elapsed().as_nanos(),
                                                conn_event.conn_id,
                                                Arc::clone(&frame),
                                            ))
                                        {
                                            error!(
                                                "send msg log error ,conid:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        if let Err(e) = self.msg_log_tx.send(
                                            MSGLOGENENT::OutShareOfferTdgwMsgInfo(
                                                now,
                                                conn_event.conn_id,
                                                Arc::clone(&frame),
                                            ),
                                        ) {
                                            error!(
                                                "send msg log error ,conn_id:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        self.session_manager
                                            .update_write_time_by_conn_id(now, id as u16);
                                    }
                                    Err(err) => {
                                        error!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={},send fail:{:?}",id,now,new_order_single,err);
                                    }
                                }
                            } else {
                                //info!(target: "benchmark::oms::in::before_reids", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                                self.session_manager.record_order(
                                    new_order_single.get_cl_ord_id().clone(),
                                    conn_event.conn_id,
                                );
                                //info!(target: "benchmark::oms::in::end_redis", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                                let tx = self.business_channels.get(&conn_event.conn_id).unwrap();

                                //info!(target: "benchmark::oms::in::before_send_channel", "conn_id={:?},time={:?},frame={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),frame);
                                // 发送消息到目标业务线程的输入通道
                                match tx.send(MsgRxEvent::NewTdgwOms2GwMsg(
                                    frame,
                                    Arc::clone(&origin_session.detail_config),
                                )) {
                                    Ok(_) => {}
                                    Err(err) => {
                                        error!("send msg error :{}", err)
                                    }
                                }
                            }
                        }
                        TdgwBinFrame::OrderCancelRequestNew(order_cancel) => {
                            //info!(target: "messages::oms::in", "conn_id={},time={},msg={}",conn_event.conn_id,now,order_cancel);

                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conn_id:{:?} error:{:?}",
                                    conn_event.conn_id, e
                                );
                            }
                            /*
                            dispatch to business logic thread
                            1. if v_software, process route logic, else not
                            2. write log
                            3. store to redis
                            */
                            // 获取目标业务线程的 Sender（从 business_channels 中获取）
                            // 待确认，是否增加对当前conn_id对于会话进行状态检查，如必须为ready状态才送至业务线程中
                            let tx = self.business_channels.get(&conn_event.conn_id).unwrap();

                            //info!(target: "benchmark::oms::in::before_send_channel", "conn_id={:?},time={:?},frame={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),frame);
                            // 发送消息到目标业务线程的输入通道
                            match tx.send(MsgRxEvent::NewTdgwOms2GwMsg(
                                frame,
                                Arc::clone(&origin_session.detail_config),
                            )) {
                                Ok(_) => {}
                                Err(err) => {
                                    error!("send msg error :{}", err)
                                }
                            }
                        }
                        TdgwBinFrame::Skip => {
                            //warn!(target: "messages::oms::in", "Skip frame,conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,frame);
                            break;
                        }
                        _ => {
                            // wrong message
                            error!(target: "messages::oms::in::error", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,frame);
                            break;
                        }
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(err) => {
                    error!("messages::oms::in::tdgw::error msg,conn_id={:?},time={:?},err:{:?}",conn_event.conn_id,now,err);
                    break;
                }
            }
        }
        //self.pipe_epoll_fd.send_wakeup();
    }

    fn handle_oms_2_tgw_msg(
        &mut self,
        rx_event: &mut TCPConnection,
        now: u128,
        conn_event: &tcp_connection::tcp_conn_event_t,
        origin_session: &Session,
        connectionState: &ConnectionProcessState,
    ) {
        loop {
            match rx_event.parse_frame::<TgwBinFrame>(connectionState) {
                Ok(Some(frame)) => {
                    // process upload stream
                    match &*frame {
                        TgwBinFrame::LogonNew(logon) => {
                            // reply logon + platform state + platform info(redis or simulate tgw?) + update self.routing_map
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            if self.session_manager.process_oms_logon_msg_tgw(
                                now,
                                conn_event.conn_id,
                                logon,
                            ) {
                                if cfg!(feature = "no_redis") {
                                    let mut partations = Vec::new();
                                    partations.push(1);

                                    if let Err(e) =
                                        self.session_manager.tgw_send_plateform_info_for_test(
                                            now,
                                            conn_event.conn_id,
                                            &partations,
                                        )
                                    {
                                        error!(
                                            "send so platrom info,conid:{:?} error:{:?},close connection",
                                            conn_event.conn_id, e
                                        );
                                        self.session_manager.set_session_status_by_conn_id(
                                            conn_event.conn_id,
                                            SessionStatus::WaitDisconnect,
                                        );
                                        continue;
                                    }
                                }
                                let so_state = self.session_manager.get_so_platform_status();
                                if let Err(e) = self.session_manager.send_so_platform_status_to_oms(
                                    conn_event.conn_id,
                                    now,
                                    so_state,
                                ) {
                                    error!(
                                        "send so platrom status,conid:{:?} error:{:?},close connection",
                                        conn_event.conn_id, e
                                    );
                                    self.session_manager.set_session_status_by_conn_id(
                                        conn_event.conn_id,
                                        SessionStatus::WaitDisconnect,
                                    );
                                    continue;
                                }

                                self.session_manager.set_session_status_by_conn_id(
                                    conn_event.conn_id,
                                    SessionStatus::Ready,
                                );
                            }
                        }
                        TgwBinFrame::LogoutNew(_) => {
                            // write log + disconnect session
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            self.session_manager.set_session_status_by_conn_id(
                                conn_event.conn_id,
                                SessionStatus::WaitDisconnect,
                            );
                        }
                        TgwBinFrame::HeartbeatNew(_) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                        }
                        TgwBinFrame::ReportSynchronizationNew(report_synchronization_new) => {
                            // write log and ignored, should send to redis
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?}",
                                    conn_event.conn_id, e
                                );
                            }
                        }
                        TgwBinFrame::NewOrder100101New(new_order_100101) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            let bench_time = self.start_time.elapsed().as_nanos();
                            if let Err(e) =
                                self.msg_log_tx.send(MSGLOGENENT::BenchmarkInOMSTgwInfo(
                                    1,
                                    bench_time,
                                    bench_time - now,
                                    conn_event.conn_id,
                                    Arc::clone(&frame),
                                ))
                            {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            //info!(target: "messages::oms::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,new_order_single);
                            //info!(target: "benchmark::oms::in::recv", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                            if (new_order_100101.user_info[0] as u16) < 48
                                || (new_order_100101.user_info[0] as u16) > 127
                            {
                                error!(
                                    "oms in new_order_100101  userinfo error! connid:{:?},msg:{},user_info:{:?}",
                                    conn_event.conn_id,
                                    new_order_100101,
                                    new_order_100101.user_info
                                );
                                continue;
                            }
                            if new_order_100101.user_info[0] as u16
                                != constants::USERINFO_FIRST_BIT_VALID_VALUE
                            {
                                //info!(target:"business::mark","new order in quick,userinfo{:?}",new_order_100101.user_info);
                                let id: u16;
                                if let Some(target_conn_id) = TCPSHARECONFIG
                                    .route_id_to_conn_id_map
                                    .get(&(new_order_100101.user_info[0] as u16))
                                {
                                    id = *target_conn_id
                                } else {
                                    error!(
                                        "oms in new_order_100101  userinfo error! connid:{:?},msg:{},user_info:{:?}",
                                        conn_event.conn_id,
                                        new_order_100101,
                                        new_order_100101.user_info
                                    );
                                    continue;
                                }
                                self.session_manager.record_order(
                                    new_order_100101.get_cl_ord_id().clone(),
                                    conn_event.conn_id,
                                );
                                let send_data = new_order_100101.as_bytes_big_endian();
                                //info!(target: "benchmark::oms::in::ready_to_send", "target_conn_id={:?},time={:?},msg={:?}",id,self.start_time.elapsed().as_nanos(),new_order_single);
                                match self
                                    .g_mgr
                                    .find_conn_by_routing(id)
                                    .tcp_conn_send_bytes(&send_data)
                                {
                                    Ok(_) => {
                                        //info!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={:?}",id,now,new_order_single);
                                        if let Err(e) =
                                            self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgwInfo(
                                                1,
                                                self.start_time.elapsed().as_nanos(),
                                                conn_event.conn_id,
                                                Arc::clone(&frame),
                                            ))
                                        {
                                            error!(
                                                "send msg log error ,conid:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        if let Err(e) = self.msg_log_tx.send(
                                            MSGLOGENENT::OutShareOfferTgwMsgInfo(
                                                now,
                                                conn_event.conn_id,
                                                Arc::clone(&frame),
                                            ),
                                        ) {
                                            error!(
                                                "send msg log error ,conid:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        self.session_manager
                                            .update_write_time_by_conn_id(now, id as u16);
                                    }
                                    Err(err) => {
                                        error!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={:?},send fail:{:?}",id,now,new_order_100101,err);
                                    }
                                }
                            } else {
                                //info!(target: "benchmark::oms::in::before_reids", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                                self.session_manager.record_order(
                                    new_order_100101.get_cl_ord_id().clone(),
                                    conn_event.conn_id,
                                );
                                //info!(target: "benchmark::oms::in::end_redis", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                                let tx = self.business_channels.get(&conn_event.conn_id).unwrap();

                                //info!(target: "benchmark::oms::in::before_send_channel", "conn_id={:?},time={:?},frame={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),frame);
                                // 发送消息到目标业务线程的输入通道
                                match tx.send(MsgRxEvent::NewTgwOms2GwMsg(
                                    frame,
                                    Arc::clone(&origin_session.detail_config),
                                )) {
                                    Ok(_) => {}
                                    Err(err) => {
                                        error!("send msg error :{}", err)
                                    }
                                }
                            }
                        }
                        TgwBinFrame::NewOrder104101New(new_order_104101) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            let bench_time = self.start_time.elapsed().as_nanos();
                            if let Err(e) =
                                self.msg_log_tx.send(MSGLOGENENT::BenchmarkInOMSTgwInfo(
                                    1,
                                    bench_time,
                                    bench_time - now,
                                    conn_event.conn_id,
                                    Arc::clone(&frame),
                                ))
                            {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            //info!(target: "messages::oms::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,new_order_single);
                            //info!(target: "benchmark::oms::in::recv", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                            if (new_order_104101.user_info[0] as u16) < 48
                                || (new_order_104101.user_info[0] as u16) > 127
                            {
                                error!(
                                    "oms in new_order_104101  userinfo error! connid:{:?},msg:{},user_info:{:?}",
                                    conn_event.conn_id,
                                    new_order_104101,
                                    new_order_104101.user_info
                                );
                                continue;
                            }
                            if new_order_104101.user_info[0] as u16
                                != constants::USERINFO_FIRST_BIT_VALID_VALUE
                            {
                                //info!(target:"business::mark","new order in quick,userinfo{:?}",new_order_104101.user_info);
                                let id: u16;
                                if let Some(target_conn_id) = TCPSHARECONFIG
                                    .route_id_to_conn_id_map
                                    .get(&(new_order_104101.user_info[0] as u16))
                                {
                                    id = *target_conn_id
                                } else {
                                    error!(
                                        "oms in new_order_100101  userinfo error! connid:{:?},msg:{},user_info:{:?}",
                                        conn_event.conn_id,
                                        new_order_104101,
                                        new_order_104101.user_info
                                    );
                                    continue;
                                }
                                let send_data = new_order_104101.as_bytes_big_endian();
                                self.session_manager.record_order(
                                    new_order_104101.get_cl_ord_id().clone(),
                                    conn_event.conn_id,
                                );
                                //info!(target: "benchmark::oms::in::ready_to_send", "target_conn_id={:?},time={:?},msg={:?}",id,self.start_time.elapsed().as_nanos(),new_order_single);
                                match self
                                    .g_mgr
                                    .find_conn_by_routing(id)
                                    .tcp_conn_send_bytes(&send_data)
                                {
                                    Ok(_) => {
                                        //info!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={:?}",id,now,new_order_single);
                                        if let Err(e) =
                                            self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgwInfo(
                                                1,
                                                self.start_time.elapsed().as_nanos(),
                                                conn_event.conn_id,
                                                Arc::clone(&frame),
                                            ))
                                        {
                                            error!(
                                                "send msg log error ,conid:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        if let Err(e) = self.msg_log_tx.send(
                                            MSGLOGENENT::OutShareOfferTgwMsgInfo(
                                                now,
                                                conn_event.conn_id,
                                                Arc::clone(&frame),
                                            ),
                                        ) {
                                            error!(
                                                "send msg log error ,conid:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        self.session_manager
                                            .update_write_time_by_conn_id(now, id as u16);
                                    }
                                    Err(err) => {
                                        error!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={:?},send fail:{:?}",id,now,new_order_104101,err);
                                    }
                                }
                            } else {
                                //info!(target: "benchmark::oms::in::before_reids", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                                self.session_manager.record_order(
                                    new_order_104101.get_cl_ord_id().clone(),
                                    conn_event.conn_id,
                                );
                                //info!(target: "benchmark::oms::in::end_redis", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),new_order_single);
                                let tx = self.business_channels.get(&conn_event.conn_id).unwrap();

                                //info!(target: "benchmark::oms::in::before_send_channel", "conn_id={:?},time={:?},frame={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),frame);
                                // 发送消息到目标业务线程的输入通道
                                match tx.send(MsgRxEvent::NewTgwOms2GwMsg(
                                    frame,
                                    Arc::clone(&origin_session.detail_config),
                                )) {
                                    Ok(_) => {}
                                    Err(err) => {
                                        error!("send msg error :{}", err)
                                    }
                                }
                            }
                        }
                        TgwBinFrame::OrderCancelRequestNew(_) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InOmsTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?}",
                                    conn_event.conn_id, e
                                );
                            }
                            /*
                            dispatch to business logic thread
                            1. if v_software, process route logic, else not
                            2. write log
                            3. store to redis
                            */
                            // 获取目标业务线程的 Sender（从 business_channels 中获取）
                            // 待确认，是否增加对当前conn_id对于会话进行状态检查，如必须为ready状态才送至业务线程中
                            let tx = self.business_channels.get(&conn_event.conn_id).unwrap();

                            //info!(target: "benchmark::oms::in::before_send_channel", "conn_id={:?},time={:?},frame={:?}",conn_event.conn_id,self.start_time.elapsed().as_nanos(),frame);
                            // 发送消息到目标业务线程的输入通道
                            match tx.send(MsgRxEvent::NewTgwOms2GwMsg(
                                frame,
                                Arc::clone(&origin_session.detail_config),
                            )) {
                                Ok(_) => {}
                                Err(err) => {
                                    error!("send msg error :{}", err)
                                }
                            }
                        }
                        TgwBinFrame::SKip => {
                            break
                        },
                        _ => {
                            // wrong message
                            error!("messages::oms::in::tgw::error,conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,frame);
                            break;
                        }
                    }
                }
                Ok(None) => {
                    //error!("messages::oms::in::None,conn_id={:?},time={:?},loop process break",conn_event.conn_id,now);
                    break;
                }
                Err(err) => {
                    error!("messages::oms::in::tgw::error,conn_id={:?},time={:?},error={:?},loop process break",conn_event.conn_id,now,err);
                    break;
                }
            }
        }
    }

    fn handle_tdgw_2_oms_msg(
        &mut self,
        rx_event: &mut TCPConnection,
        now: u128,
        conn_event: &tcp_connection::tcp_conn_event_t,
        origin_session: &Session,
        connectionState: &ConnectionProcessState,
    ) {
        loop {
            match &rx_event.parse_frame::<TdgwBinFrame>(connectionState) {
                Ok(Some(frame)) => {
                    // process download stream
                    match &**frame {
                        TdgwBinFrame::LogonNew(logon) => {
                            //MsgType=40 write log
                            //info!(target: "messages::tdgw::in", "conn_id={},time={},msg={}",conn_event.conn_id,now,logon);
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            self.session_manager.set_session_status_by_conn_id(
                                conn_event.conn_id,
                                SessionStatus::LoggedIn,
                            );
                            if !cfg!(feature = "no_redis") {
                                if let Some(ev) =
                                    self.session_manager.build_gw_info_event(conn_event.conn_id)
                                {
                                    if let Some(tx) = &self.redis_write_tx {
                                        let _ = tx.send(ev);
                                    }
                                }
                            }
                        }
                        TdgwBinFrame::LogoutNew(logout) => {
                            //MsgType=41 disconnect session
                            //info!(target: "messages::tdgw::in", "conn_id={},time={},msg={}",conn_event.conn_id,now,logout);
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            self.session_manager.set_session_status_by_conn_id(
                                conn_event.conn_id,
                                SessionStatus::WaitDisconnect,
                            );
                        }
                        TdgwBinFrame::HeartbeatNew(heartbeat) => {
                            //MsgType=33 process tgw hb
                            //info!(target: "messages::tdgw::in", "conn_id={},time={},msg={}",conn_event.conn_id,now,heartbeat);
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                        }
                        TdgwBinFrame::ExecutionReportResponseNew(_)
                        | TdgwBinFrame::ExecutionReportNew(_)
                        | TdgwBinFrame::CancelRejectNew(_)
                        | TdgwBinFrame::OrderRejectNew(_) => {
                            //todo build_store_event
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            match &**frame {
                                TdgwBinFrame::ExecutionReportResponseNew(report) => {
                                    //info!(target: "messages::tdgw::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,report);
                                    let pbu = report.get_pbu();
                                    let set_id = report.get_set_id();
                                    let report_index = report.get_report_index();
                                    let full_report_data = report.as_bytes_big_endian();
                                    let pbu_str = String::from_utf8_lossy(pbu).trim().to_string();
                                    if !cfg!(feature = "no_redis") {
                                        if let Some(event) = self.session_manager.build_store_event(
                                            conn_event.conn_id,
                                            &pbu_str,
                                            set_id as u32,
                                            report_index,
                                            full_report_data,
                                        ) {
                                            if let Some(tx) = &self.redis_write_tx {
                                                if let Err(e) = tx.send(event) {
                                                    error!(target: "system", "Failed to send ExecutionReportResponse to Redis write thread: {:?}", e);
                                                }
                                            }
                                        }
                                        self.session_manager.record_partition_routing_from_report(
                                            &pbu_str,
                                            set_id as u32,
                                            conn_event.conn_id,
                                        );
                                    }
                                }
                                TdgwBinFrame::ExecutionReportNew(report) => {
                                    //info!(target: "messages::tdgw::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,report);
                                    let pbu = report.get_pbu();
                                    let set_id = report.get_set_id();
                                    let report_index = report.get_report_index();
                                    let full_report_data = report.as_bytes_big_endian();
                                    let pbu_str = String::from_utf8_lossy(pbu).trim().to_string();
                                    if !cfg!(feature = "no_redis") {
                                        if let Some(event) = self.session_manager.build_store_event(
                                            conn_event.conn_id,
                                            &pbu_str,
                                            set_id as u32,
                                            report_index,
                                            full_report_data,
                                        ) {
                                            if let Some(tx) = &self.redis_write_tx {
                                                if let Err(e) = tx.send(event) {
                                                    error!(target: "system", "Failed to send ExecutionReportNew to Redis write thread: {:?}", e);
                                                }
                                            }
                                        }
                                    }

                                    self.session_manager.record_partition_routing_from_report(
                                        &pbu_str,
                                        set_id as u32,
                                        conn_event.conn_id,
                                    );
                                }
                                TdgwBinFrame::CancelRejectNew(report) => {
                                    //info!(target: "messages::tdgw::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,report);
                                    let pbu = report.get_pbu();
                                    let set_id = report.get_set_id();
                                    let report_index = report.get_report_index();
                                    let full_report_data = report.as_bytes_big_endian();
                                    let pbu_str = String::from_utf8_lossy(pbu).trim().to_string();
                                    if !cfg!(feature = "no_redis") {
                                        if let Some(event) = self.session_manager.build_store_event(
                                            conn_event.conn_id,
                                            &pbu_str,
                                            set_id as u32,
                                            report_index,
                                            full_report_data,
                                        ) {
                                            if let Some(tx) = &self.redis_write_tx {
                                                if let Err(e) = tx.send(event) {
                                                    error!(target: "system", "Failed to send CancelReject to Redis write thread: {:?}", e);
                                                }
                                            }
                                        }
                                    }

                                    self.session_manager.record_partition_routing_from_report(
                                        &pbu_str,
                                        set_id as u32,
                                        conn_event.conn_id,
                                    );
                                }
                                TdgwBinFrame::OrderRejectNew(order_reject) => {
                                    //info!(target: "messages::tdgw::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,order_reject);
                                }
                                _ => unreachable!("can't be reach！"),
                            }
                            let received_time = self.start_time.elapsed().as_nanos();
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::BenchmarkInTdgwInfo(
                                1,
                                received_time,
                                received_time - now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }

                            /*
                            dispatch to business logic thread
                            1. if v_software, process route logic, else not
                            2. write log
                            3. store to redis
                            */
                            // 计算目标业务线程索引（哈希轮询）
                            // hash by contract_num -> ensure execution report seq (wth by wth contract_num?)
                            /*let target_thread_idx =
                                Self::hash_round_robin(
                                    &frame,
                                    NUM_BUSINESS_THREADS,
                                );
                            */

                            // 获取目标业务线程的 Sender（从 business_channels 中获取）
                            let tx = self.business_channels.get(&conn_event.conn_id).unwrap();

                            // 发送消息到目标业务线程的输入通道
                            match tx.send(MsgRxEvent::NewTdgwGw2OmsMsg(
                                Arc::clone(&frame),
                                Arc::clone(&origin_session.detail_config),
                            )) {
                                Ok(_) => {}
                                Err(err) => {
                                    error!(
                                        "tdgw 回报业务线程发送失败！！error:{:?}msg:{:?}",
                                        err, frame
                                    )
                                }
                            }
                            //println!("分发线程 发送消息 {} → 业务线程 {}", msg, target_thread_idx);
                        }
                        TdgwBinFrame::ExecRptSyncRspNew(exec_rpt_sync_rsp) => {
                            //MsgType = 207  write log + update memory
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            //info!(target: "messages::tdgw::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,exec_rpt_sync_rsp);
                            self.session_manager.process_tdgw_exec_rpt_sync_rsp_msg(
                                conn_event.conn_id,
                                &exec_rpt_sync_rsp,
                            );
                        }
                        TdgwBinFrame::ExecRptInfoNew(execrptinfo) => {
                            //MsgType = 208 send report sync, read from redis
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            //info!(target: "messages::tdgw::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,execrptinfo);
                            self.session_manager.process_tdgw_exec_rpt_info_msg(
                                now,
                                conn_event.conn_id,
                                &execrptinfo,
                            );
                        }
                        TdgwBinFrame::PlatformStateNew(platformstate) => {
                            //MsgType = 209  write log + update memory + set session ready, allow to send order +update gw route
                            // info!(target: "messages::tdgw::in", "messages::tdgw::in, conn_id={:?},time={:?},msg={:?}",
                            //       conn_event.conn_id, now, platformstate);
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            let (_, has_gw_reay) =
                                self.session_manager.process_gw_platform_state_msg(
                                    now,
                                    conn_event.conn_id,
                                    platformstate.get_platform_id(),
                                    platformstate.get_platform_state(),
                                );
                            if !cfg!(feature = "no_redis") {
                                if let Some(ev) = self.session_manager.build_gw_status_event(
                                    conn_event.conn_id,
                                    platformstate.get_platform_id(),
                                    platformstate.get_platform_state(),
                                ) {
                                    if let Some(tx) = &self.redis_write_tx {
                                        let _ = tx.send(ev);
                                    }
                                }
                            }

                            // 如果有新ready的gw session，更新网关路由表
                            if has_gw_reay {
                                self.update_gw_routing_table();
                            }
                        }
                        TdgwBinFrame::ExecRptEndOfStreamNew(exec_rpt_end_of_stream) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTdgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            //MsgType = 210  write log
                            //info!(target: "messages::tdgw::in", "conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,exec_rpt_end_of_stream);
                        }
                        TdgwBinFrame::Skip => {
                            warn!(target: "messages::tdgw::in", "Skip frame,conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,frame);
                        }
                        _ => {
                            // wrong message
                            warn!(target: "messages::tddgw::in", "wrong frame,conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,frame);
                        }
                    }
                }
                Ok(None) => {
                    //warn!(target: "messages::tdgw::in", "wrong none msg,conn_id={:?},time={:?}",conn_event.conn_id,now);
                    break;
                }
                Err(err) => {
                    error!("messages::tdgw::in::error msg,conn_id={:?},time={:?},err:{:?}",conn_event.conn_id,now,err);
                    break;
                }
            }
        }
    }

    fn handle_tgw_2_oms_msg(
        &mut self,
        rx_event: &mut TCPConnection,
        now: u128,
        conn_event: &tcp_connection::tcp_conn_event_t,
        origin_session: &Session,
        connectionState: &ConnectionProcessState,
    ) {
        loop {
            match rx_event.parse_frame::<TgwBinFrame>(connectionState) {
                Ok(Some(frame)) => {
                    // process download stream
                    match &*frame {
                        TgwBinFrame::LogonNew(_) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            self.session_manager.set_session_status_by_conn_id(
                                conn_event.conn_id,
                                SessionStatus::LoggedIn,
                            );
                        }
                        TgwBinFrame::LogoutNew(_) => {
                            // disconnect session
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            self.session_manager.set_session_status_by_conn_id(
                                conn_event.conn_id,
                                SessionStatus::WaitDisconnect,
                            );
                        }
                        TgwBinFrame::HeartbeatNew(_) => {
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                        }
                        TgwBinFrame::BusinessRejectNew(business_reject) => {
                            let received_time = self.start_time.elapsed().as_nanos();
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::BenchmarkInTgwInfo(
                                1,
                                received_time,
                                received_time - now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            if let Some(target_oms_conn_id) = self
                                .session_manager
                                .route_report(business_reject.get_business_reject_ref_id())
                            {
                                match self
                                    .g_mgr
                                    .find_conn_by_routing(target_oms_conn_id)
                                    .tcp_conn_send_bytes(&business_reject.as_bytes_big_endian())
                                {
                                    Ok(_) => {
                                        //info!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={:?}",id,now,new_order_single);
                                        if let Err(e) =
                                            self.msg_log_tx.send(MSGLOGENENT::BenchmarkOutTgwInfo(
                                                1,
                                                self.start_time.elapsed().as_nanos(),
                                                target_oms_conn_id,
                                                Arc::clone(&frame),
                                            ))
                                        {
                                            error!(
                                                "send msg log error ,conid:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        if let Err(e) = self.msg_log_tx.send(
                                            MSGLOGENENT::OutShareOfferTgwMsgInfo(
                                                now,
                                                target_oms_conn_id,
                                                Arc::clone(&frame),
                                            ),
                                        ) {
                                            error!(
                                                "send msg log error ,conid:{:?} error:{:?},close connection",
                                                conn_event.conn_id, e
                                            );
                                        }
                                        self.session_manager.update_write_time_by_conn_id(
                                            now,
                                            target_oms_conn_id as u16,
                                        );
                                    }
                                    Err(err) => {
                                        error!(target: "messages::share_offer::out", "target_conn_id={:?},time={:?},msg={},send fail:{:?}",target_oms_conn_id,now,business_reject,err);
                                    }
                                }
                            } else {
                                error!(
                                    "tgw business reject error! could not found! conn_id={},msg={}",
                                    conn_event.conn_id, business_reject
                                );
                            }
                        }
                        TgwBinFrame::CancelRejectNew(_)
                        | TgwBinFrame::ExecutionReportResponse200102New(_)
                        | TgwBinFrame::ExecutionReportResponse200202New(_)
                        | TgwBinFrame::ExecutionReportResponse204102New(_)
                        | TgwBinFrame::ExecutionReport200115New(_) 
                        | TgwBinFrame::ExecutionReport200215New(_)
                        | TgwBinFrame::ExecutionReport204115New(_)
                        => {
                            let received_time = self.start_time.elapsed().as_nanos();
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::BenchmarkInTgwInfo(
                                1,
                                received_time,
                                received_time - now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }

                            /*
                            dispatch to business logic thread
                            1. if v_software, process route logic, else not
                            2. write log
                            3. store to redis
                            */
                            // 计算目标业务线程索引（哈希轮询）
                            // hash by contract_num -> ensure execution report seq (wth by wth contract_num?)
                            /*let target_thread_idx =
                                Self::hash_round_robin(
                                    &frame,
                                    NUM_BUSINESS_THREADS,
                                );
                            */

                            // 获取目标业务线程的 Sender（从 business_channels 中获取）
                            let tx = self.business_channels.get(&conn_event.conn_id).unwrap();

                            // 发送消息到目标业务线程的输入通道
                            match tx.send(MsgRxEvent::NewTgwGw2OmsMsg(
                                Arc::clone(&frame),
                                Arc::clone(&origin_session.detail_config),
                            )) {
                                Ok(_) => {}
                                Err(err) => {
                                    error!(
                                        "tdgw 回报业务线程发送失败！！error:{:?}msg:{:?}",
                                        err, frame
                                    )
                                }
                            }
                            //println!("分发线程 发送消息 {} → 业务线程 {}", msg, target_thread_idx);
                        }
                        TgwBinFrame::PlatformStateInfoNew(platform_state_info) => {
                            // write log + update memory + allow to send order
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }

                            let (_, has_gw_reay) =
                                self.session_manager.process_gw_platform_state_msg(
                                    now,
                                    conn_event.conn_id,
                                    platform_state_info.get_platform_id(),
                                    platform_state_info.get_platform_state(),
                                );
                            // 如果有新ready的gw session，更新网关路由表
                            if has_gw_reay {
                                self.update_gw_routing_table();
                            }
                        }
                        TgwBinFrame::ReportFinishedNew(_) => {
                            // write log
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                        }
                        TgwBinFrame::PlatformInfoNew(platform_info) => {
                            // send report sync, read from redis
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                            // let mut partations = Vec::new();
                            // for each_partation in platform_info.get_no_partitions_3(){
                            //     partations.push(each_partation.get_partition_no());
                            // }
                            // if let Err(e) = self
                            //         .session_manager
                            //         .tgw_send_report_synchronization_for_test(now, conn_event.conn_id,&partations)
                            //     {
                            //         error!(
                            //             "tgw_send_report_synchronization_for_test error:{:?},close connection",
                            //             e
                            //         );
                            // }
                        }
                        TgwBinFrame::TradingSessionStatusNew(_) => {
                            // write log
                            if let Err(e) = self.msg_log_tx.send(MSGLOGENENT::InTgwMsgInfo(
                                now,
                                conn_event.conn_id,
                                Arc::clone(&frame),
                            )) {
                                error!(
                                    "send msg log error ,conid:{:?} error:{:?},close connection",
                                    conn_event.conn_id, e
                                );
                            }
                        }
                        TgwBinFrame::SKip => {
                            warn!(target: "messages::tgw::in", "Skip frame,conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,frame);
                        }
                        _ => {
                            // wrong message
                            warn!(target: "messages::tgw::in", "error frame,conn_id={:?},time={:?},msg={:?}",conn_event.conn_id,now,frame);
                        }
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(err) => {
                    error!("messages::tgw::in::error msg,conn_id={:?},time={:?},err:{:?}",conn_event.conn_id,now,err);
                    break;
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_share_offer_init() {
        let mut share_offer = ShareOffer::new();
        share_offer.init();
        share_offer.run();
    }
}
