use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Authentication failed: check your username or password")]
    AuthFailed,

    #[error("password-required")]
    PasswordRequired,

    #[error("Local shell error: {0}")]
    Pty(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),
}

// Command errors are delivered to the frontend as message strings.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
