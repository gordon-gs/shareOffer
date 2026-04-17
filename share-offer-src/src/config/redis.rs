use crate::startup_result::StartUpResult;
use config::{Config, File, FileFormat};
use lazy_static::lazy_static;
use serde::Deserialize;

lazy_static! {
    pub static ref REDISCONFIG: RedisConfig =
        RedisConfig::init().expect("Redis config init failed");
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub cluster_nodes: Vec<String>,
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_ms: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_interval")]
    pub retry_interval_ms: u64,
}

fn default_connection_timeout() -> u64 {
    5000
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_interval() -> u64 {
    1000
}

impl RedisConfig {
    pub fn init() -> StartUpResult<Self> {
        let config = Config::builder()
            .add_source(File::with_name("config/redis").format(FileFormat::Json))
            .build()?;
        let result: Self = config.try_deserialize()?;

        if result.cluster_nodes.is_empty() {
             return Err(anyhow::anyhow!("Redis cluster_nodes cannot be empty").into());
        }
        Ok(result)
    }

    pub fn init_from_file(file_path: String, file_fmt: FileFormat) -> StartUpResult<Self> {
        let config = Config::builder()
            .add_source(File::with_name(&file_path).format(file_fmt))
            .build()?;
        let result: Self = config.try_deserialize()?;

        if result.cluster_nodes.is_empty() {
            return Err(anyhow::anyhow!("Redis cluster_nodes cannot be empty").into());
        }
        Ok(result)
    }
}
