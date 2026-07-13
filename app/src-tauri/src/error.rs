use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("pack error: {0}")]
    Pack(String),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Cohere API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("monthly API quota exhausted")]
    QuotaExhausted,
    #[error("no API key configured")]
    NoApiKey,
    #[error("key store error: {0}")]
    Keyring(String),
    #[error("{0}")]
    Internal(String),
}

/// Shape errors cross the IPC boundary in: a stable kind for the UI to switch
/// on plus a human-readable message.
#[derive(Serialize)]
struct ErrorPayload<'a> {
    kind: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'a u16>,
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let kind = match self {
            Error::Io(_) => "io",
            Error::Db(_) => "db",
            Error::Pack(_) => "pack",
            Error::Http(_) => "network",
            Error::Api { .. } => "api",
            Error::QuotaExhausted => "quota_exhausted",
            Error::NoApiKey => "no_api_key",
            Error::Keyring(_) => "keyring",
            Error::Internal(_) => "internal",
        };
        let status = match self {
            Error::Api { status, .. } => Some(status),
            _ => None,
        };
        ErrorPayload { kind, message: self.to_string(), status }.serialize(serializer)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
