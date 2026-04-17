use crate::startup_result::StartUpResult;
use config::{Config, File, FileFormat};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;

lazy_static! {
    pub static ref TGWCONFIG: TgwConfig = TgwConfig::init().expect("system init failed");
}
#[derive(Debug, Deserialize,Default,Clone)]
pub struct TgwConfig {
    #[serde(default)]
    pub session: Vec<TgwSession>,
    pub reconnect_interval: u16,
    pub heart_bt_int: i32,
    pub default_appl_ver_id: String,
    #[serde(skip)]
    pub connect_string_to_session_map: HashMap<String, TgwSession>,
    #[serde(skip)]
    pub session_id_to_session_map: HashMap<String, TgwSession>,
}

#[derive(Debug, Deserialize, Clone,PartialEq)]
pub struct TgwSession {
    pub sender_comp_id: String,
    pub target_comp_id: String,
    pub pbus: Vec<String>,
    pub platform_id: u16,
    pub socket_connect_port: u32,
    pub socket_connect_host: String,
    pub password: String,
}

impl TgwConfig {
    pub fn init() -> StartUpResult<Self> {
        let format: FileFormat = if cfg!(feature = "config_json") {
            FileFormat::Json
        } else {
            FileFormat::Toml
        };
        let config = Config::builder()
            .add_source(File::with_name("config/tgw").format(format))
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

    pub fn init_from_file(file_path: String, file_format: FileFormat) -> StartUpResult<Self> {
        let config = Config::builder()
            .add_source(File::with_name(&file_path).format(file_format))
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
    fn test_tgw_init() {
        println!();
        let res_toml = TgwConfig::init_from_file(String::from("config/tgw"), FileFormat::Toml);
        match res_toml {
            Ok(test_config) => {
                println!("toml_config:{:?}", test_config);
                assert_eq!(test_config.heart_bt_int, 20);
            }
            Err(err) => {
                panic!("{:?}", err)
            }
        }
        let res_json = TgwConfig::init_from_file(String::from("config/tgw"), FileFormat::Json);
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
