use thiserror::Error;

#[derive(Error, Debug)]
pub enum MarkitsError {
    #[error("Serialization/Deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Layout error: {0}")]
    Layout(String),

    #[error("Rendering error: {0}")]
    Render(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MarkitsError>;
