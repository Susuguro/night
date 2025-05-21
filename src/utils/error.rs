use log::SetLoggerError;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NightError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Task error: {0}")]
    Task(String),

    #[error("Mission error: {0}")]
    Mission(String),

    #[error("Communication error: {0}")]
    Communication(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("YAML Serialization error: {0}")]
    YamlSerialization(#[from] serde_yaml::Error),

    #[error("Logging error: {0}")]
    Logging(#[from] SetLoggerError),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, NightError>;
