pub mod local;
pub mod logger;
pub mod serial;
pub mod ssh;
pub mod telnet;
mod xymodem;

pub use xymodem::Protocol as TransferProtocol;

pub use logger::SessionLogger;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tokio::sync::mpsc;

use crate::error::{Error, Result};

/// Connection settings. Maps 1:1 to frontend profiles via serde tag.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Profile {
    Serial {
        port: String,
        baud_rate: u32,
    },
    Ssh {
        host: String,
        port: u16,
        username: String,
    },
    Telnet {
        host: String,
        port: u16,
        username: Option<String>,
    },
    /// Local shell on a ConPTY (WSL, PowerShell, cmd, …)
    Local {
        command: String,
        /// Working directory; empty/None uses the per-shell default
        cwd: Option<String>,
    },
}

/// Backend → frontend stream events (delivered via tauri ipc Channel)
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputEvent {
    Data { bytes: Vec<u8> },
    Connected,
    Disconnected { reason: String },
    /// An XMODEM/YMODEM transfer began (after the local file was read successfully)
    TransferStart { name: String, size: u64 },
    /// The receiver answered and the first block is going out: the UI can
    /// stop saying "waiting for the receiver" before the first ACK, which a
    /// receiver that erases flash on block 1 may take a second to send
    TransferHandshake,
    TransferProgress { sent: u64, size: u64 },
    TransferDone { name: String },
    /// `reason` is a stable token (cancelled, remote-cancelled,
    /// handshake-timeout, transfer-timeout) or raw error text
    TransferFailed { reason: String },
}

/// Frontend → session input
#[derive(Debug)]
pub enum SessionInput {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    /// Serial only: the user is picking a file to send. Port input is held
    /// back from the terminal until TransferSend (which takes the receiver's
    /// poll byte from it) or TransferCancel (which releases it) arrives, the
    /// way Tera Term's frozen screen keeps the `C` for its XMODEM sender.
    TransferPrepare,
    /// Serial only: send the file at `path` to the target via XMODEM or
    /// YMODEM. The target's receiver (rx/rb, loadx/loady, …) must already be
    /// running.
    TransferSend {
        path: String,
        protocol: TransferProtocol,
    },
    /// Abort the XMODEM/YMODEM transfer in progress, or drop a
    /// TransferPrepare hold when the file dialog was dismissed
    TransferCancel,
    Close,
}

struct SessionEntry {
    tx: mpsc::Sender<SessionInput>,
    logger: SessionLogger,
}

type SessionMap = Arc<Mutex<HashMap<String, SessionEntry>>>;

/// Handle for a session task to remove itself from the manager when it ends
pub struct SessionCleanup {
    id: String,
    sessions: SessionMap,
}

impl SessionCleanup {
    pub fn run(&self) {
        self.sessions.lock().unwrap().remove(&self.id);
    }
}

#[derive(Default)]
pub struct SessionManager {
    sessions: SessionMap,
}

impl SessionManager {
    pub async fn open(
        &self,
        profile: Profile,
        password: Option<String>,
        cols: u16,
        rows: u16,
        output: Channel<OutputEvent>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel::<SessionInput>(64);
        let cleanup = SessionCleanup {
            id: id.clone(),
            sessions: Arc::clone(&self.sessions),
        };
        let logger = SessionLogger::default();

        match profile {
            Profile::Serial { port, baud_rate } => {
                serial::spawn(port, baud_rate, rx, output, cleanup, logger.clone())?;
            }
            Profile::Ssh {
                host,
                port,
                username,
            } => {
                ssh::spawn(
                    host,
                    port,
                    username,
                    password.unwrap_or_default(),
                    cols,
                    rows,
                    rx,
                    output,
                    cleanup,
                    logger.clone(),
                )
                .await?;
            }
            Profile::Telnet {
                host,
                port,
                username,
            } => {
                telnet::spawn(
                    host,
                    port,
                    username,
                    cols,
                    rows,
                    rx,
                    output,
                    cleanup,
                    logger.clone(),
                )
                .await?;
            }
            Profile::Local { command, cwd } => {
                local::spawn(command, cwd, cols, rows, rx, output, cleanup, logger.clone())?;
            }
        }

        self.sessions
            .lock()
            .unwrap()
            .insert(id.clone(), SessionEntry { tx, logger });
        Ok(id)
    }

    /// Start writing this session's output to `path` (replaces any active log)
    pub fn start_log(&self, id: &str, path: &str, timestamps: bool, plain: bool) -> Result<()> {
        let logger = self
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .map(|e| e.logger.clone())
            .ok_or_else(|| Error::SessionNotFound(id.to_string()))?;
        logger.start(path, timestamps, plain)
    }

    pub fn stop_log(&self, id: &str) {
        if let Some(entry) = self.sessions.lock().unwrap().get(id) {
            entry.logger.stop();
        }
    }

    pub async fn send(&self, id: &str, input: SessionInput) -> Result<()> {
        let tx = self
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .map(|e| e.tx.clone())
            .ok_or_else(|| Error::SessionNotFound(id.to_string()))?;
        tx.send(input)
            .await
            .map_err(|_| Error::SessionNotFound(id.to_string()))
    }

    pub async fn close(&self, id: &str) -> Result<()> {
        let entry = self.sessions.lock().unwrap().remove(id);
        if let Some(entry) = entry {
            entry.logger.stop();
            let _ = entry.tx.send(SessionInput::Close).await;
        }
        Ok(())
    }
}
