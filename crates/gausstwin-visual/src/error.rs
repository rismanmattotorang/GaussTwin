use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Dashboard error: {0}")]
    Dashboard(String),

    #[error("Analytics error: {0}")]
    Analytics(String),

    #[error("Scenario error: {0}")]
    Scenario(String),

    #[error("Server error: {0}")]
    Server(String),

    #[error("Invalid configuration: {0}")]
    Config(String),
}
