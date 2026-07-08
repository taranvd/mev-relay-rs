use thiserror::Error;

#[derive(Debug, Error)]
pub enum BeaconError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: status={status}, body={body}")]
    Api { status: u16, body: String },
    #[error("Deserialization error: {0}")]
    Deserialize(reqwest::Error),
    #[error("SSE error: {0}")]
    Sse(String),
    #[error("Channel error: {0}")]
    Channel(String),
}
