use crate::startup_result::StartUpResult;
use config::{Config, File, FileFormat};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;
use crate::config::tgw::TgwSession;

lazy_static! {
    pub static ref TDGWCONFIG: TdgwConfig = TdgwConfig::init().expect("system init failed");
}
#[derive(Debug, Deserialize,Default,Clone)]
pub struct TdgwConfig {
    #[serde(default)]
    pub session: Vec<TdgwSession>,
    pub reconnect_interval: u16,
    pub heart_bt_int: i32,
    pub prtcl_version: String,
    #[serde(skip)]
    pub connect_string_to_session_map: HashMap<String, TdgwSession>,
    #[serde(skip)]
    pub session_id_to_session_map: HashMap<String, TdgwSession>,
}

#[derive(Debug, Deserialize, Clone,PartialEq)]
pub struct TdgwSession {
    pub sender_comp_id: String,
    pub target_comp_id: String,
    pub pbus: Vec<String>,
    pub platform_id: u16,
    pub socket_connect_port: u32,
    pub socket_connect_host: String,
}

impl TdgwConfig {
    pub fn init() -> StartUpResult<Self> {
        let format: FileFormat = if cfg!(feature = "config_json") {
            FileFormat::Json
        } else {
            FileFormat::Toml
        };
        let config = Config::builder()
            .add_source(File::with_name("config/tdgw").format(format))
            .build()?;
        let mut result: Self = config.try_deserialize()?;
        for session in &result.session {
            result.connect_string_to_session_map.insert(
                format!(
                    "{}:{}",
                    session.socket_connect_host, session.socket_connect_port
                ),
                session.clone(),
            );
        }
        for session in &result.session {
            result.session_id_to_session_map.insert(
                format!(
                    "{}",
                    session.target_comp_id
                ),
                session.clone(),
            );
        }
        Ok(result)
    }
    pub fn init_from_file(file_path: String, file_fmt: FileFormat) -> StartUpResult<Self> {
        let config = Config::builder()
            .add_source(File::with_name(&file_path).format(file_fmt))
            .build()?;
        let mut result: Self = config.try_deserialize()?;
        for session in &result.session {
            result.connect_string_to_session_map.insert(
                format!(
                    "{}:{}",
                    session.socket_connect_host, session.socket_connect_port
                ),
                session.clone(),
            );
        }
        for session in &result.session {
            result.session_id_to_session_map.insert(
                format!(
                    "{}",
                    session.target_comp_id
                ),
                session.clone(),
            );
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_tdgw_init() {
        let res_toml = TdgwConfig::init_from_file(String::from("config/tdgw"), FileFormat::Toml);
        println!();
        match res_toml {
            Ok(test_config) => {
                println!("toml_cofig:{:?}", test_config);
                assert_eq!(test_config.heart_bt_int, 20);
            }
            Err(err) => {
                panic!("{:?}", err)
            }
        }
        let res_json = TdgwConfig::init_from_file(String::from("config/tdgw"), FileFormat::Json);
        match res_json {
            Ok(test_config) => {
                println!("json_cofig:{:?}", test_config);
                assert_eq!(test_config.heart_bt_int, 20);
            }
            Err(err) => {
                panic!("{:?}", err)
            }
        }
    }
}
