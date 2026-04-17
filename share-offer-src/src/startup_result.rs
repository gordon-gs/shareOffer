use thiserror::Error;

pub type StartUpResult<T> = Result<T, StartUpError>;

#[derive(Error, Debug)]
pub enum StartUpError {
    #[error("config error")]
    ConfigError {
        #[from]
        source: config::ConfigError,
    },
    #[error("oracle error")]
    OracleError {
        #[from]
        source: oracle::Error,
    },
    #[error("csv error")]
    CsvError {
        #[from]
        source: csv::Error,
    },
    #[error("io error")]
    IO {
        #[from]
        source: std::io::Error,
    },
    // #[error("ExtremeDb Error, error code: {0}, description: {1}")]
    // ExtremeDBError(RcType, String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
