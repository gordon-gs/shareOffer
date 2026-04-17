use crate::startup_result::StartUpResult;
use config::{Config, File, FileFormat};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;

lazy_static! {
    pub static ref OMSCONFIG: OMSConfig = OMSConfig::init().expect("system init failed");
}

#[derive(Deserialize, Debug, PartialEq,Clone)]
pub enum GWType {
    TDGW,
    TGW
}

#[derive(Debug, Deserialize,Default,Clone)]
pub struct OMSConfig {
    #[serde(default)]
    pub session: Vec<OMSSession>,
    pub reconnect_interval: u16,
    pub heart_bt_int: i32,
    pub default_appl_ver_id: String,
    #[serde(default)]
    pub ip_whitelist: Vec<String>,
    #[serde(skip)]
    pub connect_string_to_session_map: HashMap<String, OMSSession>,
    #[serde(skip)]
    pub server_id_to_session_map: HashMap<String, OMSSession>, //反查map
}

#[derive(Debug, Deserialize, Clone,PartialEq)]
pub struct OMSSession {
    pub server_id: String,
    pub socket_connect_host: String,
    pub socket_connect_port: u16,
    pub platform_id: u16,
    pub gw_type: GWType

}

impl OMSConfig {
    pub fn init() -> StartUpResult<Self> {
        let format: FileFormat = if cfg!(feature = "config_json") {
            FileFormat::Json
        } else {
            FileFormat::Toml
        };
        let config = Config::builder()
            .add_source(File::with_name("config/oms").format(format))
            .build()?;
        let mut result: Self = config.try_deserialize()?;
        for session in &result.session {
            result
                .connect_string_to_session_map
                .insert(format!("{}", session.socket_connect_host), session.clone());
            result
                .server_id_to_session_map
                .insert(format!("{}", session.server_id), session.clone());
        }
        Ok(result)
    }
    pub fn init_from_file(file_path: String, file_fmt: FileFormat) -> StartUpResult<Self> {
        let config = Config::builder()
            .add_source(File::with_name(&file_path).format(file_fmt))
            .build()?;
        let mut result: Self = config.try_deserialize()?;
        for session in &result.session {
            result
                .server_id_to_session_map
                .insert(format!("{}", session.server_id), session.clone());
        }
        Ok(result)
    }

    pub fn is_ip_allowed(&self, ip: &str) -> bool {
        if self.ip_whitelist.is_empty() {
            return true;
        }
        self.ip_whitelist.contains(&ip.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_oms_init() {
        let res_toml = OMSConfig::init_from_file(String::from("config/oms"), FileFormat::Toml);
        println!();
        match res_toml {
            Ok(test_config) => {
                println!("toml_config:{:?}", test_config);
                assert_eq!(test_config.heart_bt_int, 20);
            }
            Err(err) => {
                panic!("{:?}", err)
            }
        }
        let res_json = OMSConfig::init_from_file(String::from("config/oms"), FileFormat::Json);
        match res_json {
            Ok(test_config) => {
                println!("json_config:{:?}", test_config);
                assert_eq!(test_config.heart_bt_int, 20);
            }
            Err(err) => {
                panic!("{:?}", err)
            }
        }
    }
}
