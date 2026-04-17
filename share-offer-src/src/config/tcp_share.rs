use crate::startup_result::StartUpResult;
use config::{Config, File, FileFormat};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;

lazy_static! {
    pub static ref TCPSHARECONFIG: TCPShareConfig =
        TCPShareConfig::init().expect("system init failed");
}
#[derive(Debug, Deserialize)]
pub struct TCPShareConfig {
    pub version: String,
    pub description: String,
    pub share_offer_id: u16,
    #[serde(rename = "type")]
    pub config_type: String,
    #[serde(default)]
    pub log_config_path: String,
    pub global_settings: GlobalSetting,
    #[serde(default)]
    pub arp_table: Vec<ArpSetting>,
    #[serde(default)]
    pub connections: Vec<ConnectionSetting>,
    #[serde(skip)]
    pub conn_tag_to_conn_map: HashMap<String, ConnectionSetting>,
    #[serde(skip)]
    pub config_file_path : String,
    #[serde(skip)]
    pub conn_id_to_route_id_map : HashMap<u16, u16>,
    #[serde(skip)]
    pub route_id_to_conn_id_map : HashMap<u16, u16>
}

#[derive(Debug, Deserialize, Clone)]
pub struct GlobalSetting {
    pub max_connections: u32,
    pub ring_buffer_size: u64,
    pub reconnect_interval: u64,
    pub connection_timeout: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArpSetting {
    #[serde(default)]
    pub host_mac: String,
    pub host_ip: String,
    #[serde(default)]
    pub is_local: bool
}

#[derive(Debug, Deserialize, Clone)]
pub struct ConnectionSetting {
    pub conn_id: u16,
    #[serde(default)]
    pub route_id: u16,
    pub conn_tag: String,
    pub conn_type: String,
    #[serde(default)]
    pub enabled: bool,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    pub remote_port: u16,
}

impl TCPShareConfig {
    pub fn init() -> StartUpResult<Self> {
        let config = Config::builder()
            .add_source(File::with_name("config/tcp_share_config").format(FileFormat::Json))
            .build()?;
        let mut result: Self = config.try_deserialize()?;
        for session in &result.connections {
            result
                .conn_tag_to_conn_map
                .insert(format!("{}", session.conn_tag), session.clone());
            result
                .conn_id_to_route_id_map
                .insert(session.conn_id,session.route_id);
            result
                .route_id_to_conn_id_map
                .insert(session.route_id,session.conn_id);
        }
        result.config_file_path = String::from("config/tcp_share_config.json");
        Ok(result)
    }
    pub fn init_from_file(file_path: String, file_fmt: FileFormat) -> StartUpResult<Self> {
        let config = Config::builder()
            .add_source(File::with_name(&file_path).format(file_fmt))
            .build()?;
        let mut result: Self = config.try_deserialize()?;
        for session in &result.connections {
            result
                .conn_tag_to_conn_map
                .insert(format!("{}", session.conn_tag), session.clone());
                        result
                .conn_id_to_route_id_map
                .insert(session.conn_id,session.route_id);
            result
                .route_id_to_conn_id_map
                .insert(session.route_id,session.conn_id);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_tcp_share_init() {
        let res_json = TCPShareConfig::init_from_file(
            String::from("config/tcp_share_config"),
            FileFormat::Json,
        );
        println!();
        match res_json {
            Ok(test_config) => {
                println!("json_config:{:?}", test_config);
                assert_eq!(test_config.version, "1.0");
            }
            Err(err) => {
                panic!("{:?}", err)
            }
        }
    }
}
