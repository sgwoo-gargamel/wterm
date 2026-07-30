use tauri::ipc::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use super::{OutputEvent, SessionCleanup, SessionInput, SessionLogger};
use crate::error::Result;

// Telnet command bytes
const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;

// Telnet options
const OPT_BINARY: u8 = 0; // 8-bit clean transmission — required for UTF-8 (e.g. Korean)
const OPT_ECHO: u8 = 1;
const OPT_SGA: u8 = 3; // Suppress Go Ahead
const OPT_TTYPE: u8 = 24; // Terminal Type
const OPT_NAWS: u8 = 31; // Negotiate About Window Size

const TTYPE_IS: u8 = 0;
const TTYPE_SEND: u8 = 1;
const TERM_NAME: &[u8] = b"xterm-256color";

#[allow(clippy::too_many_arguments)]
pub async fn spawn(
    host: String,
    port: u16,
    username: Option<String>,
    cols: u16,
    rows: u16,
    rx: mpsc::Receiver<SessionInput>,
    output: Channel<OutputEvent>,
    cleanup: SessionCleanup,
    logger: SessionLogger,
) -> Result<()> {
    let stream = TcpStream::connect((host.as_str(), port)).await?;
    let _ = stream.set_nodelay(true);
    let _ = output.send(OutputEvent::Connected);

    tauri::async_runtime::spawn(async move {
        let reason = match session_loop(stream, username, cols, rows, rx, &output, &logger).await {
            Ok(reason) => reason,
            Err(e) => e.to_string(),
        };
        logger.stop();
        let _ = output.send(OutputEvent::Disconnected { reason });
        cleanup.run();
    });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn session_loop(
    stream: TcpStream,
    username: Option<String>,
    cols: u16,
    rows: u16,
    mut rx: mpsc::Receiver<SessionInput>,
    output: &Channel<OutputEvent>,
    logger: &SessionLogger,
) -> Result<String> {
    let (mut reader, mut writer) = stream.into_split();
    let mut parser = Parser::new(cols, rows, username);
    let mut buf = [0u8; 4096];

    loop {
        tokio::select! {
            n = reader.read(&mut buf) => {
                let n = n?;
                if n == 0 {
                    return Ok("remote-closed".into());
                }
                let (data, replies) = parser.feed(&buf[..n]);
                if !replies.is_empty() {
                    writer.write_all(&replies).await?;
                }
                if !data.is_empty() {
                    logger.write(&data);
                    let _ = output.send(OutputEvent::Data { bytes: data });
                }
            }
            input = rx.recv() => {
                match input {
                    Some(SessionInput::Data(data)) => {
                        writer.write_all(&escape_iac(&data)).await?;
                    }
                    Some(SessionInput::Resize { cols, rows }) => {
                        parser.cols = cols;
                        parser.rows = rows;
                        if parser.naws_enabled {
                            writer.write_all(&naws_subneg(cols, rows)).await?;
                        }
                    }
                    Some(SessionInput::Close) | None => {
                        return Ok("closed".into());
                    }
                }
            }
        }
    }
}

/// Escape 0xFF in user data as IAC IAC
fn escape_iac(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        out.push(b);
        if b == IAC {
            out.push(IAC);
        }
    }
    out
}

fn naws_subneg(cols: u16, rows: u16) -> Vec<u8> {
    let mut out = vec![IAC, SB, OPT_NAWS];
    for b in [
        (cols >> 8) as u8,
        (cols & 0xff) as u8,
        (rows >> 8) as u8,
        (rows & 0xff) as u8,
    ] {
        out.push(b);
        if b == IAC {
            out.push(IAC);
        }
    }
    out.extend_from_slice(&[IAC, SE]);
    out
}

enum State {
    Normal,
    Iac,
    Negotiate(u8), // waiting for the option byte after WILL/WONT/DO/DONT
    Sub,           // waiting for the subnegotiation option byte
    SubData(u8),   // collecting subnegotiation data
    SubIac(u8),    // IAC received during subnegotiation
}

struct Parser {
    state: State,
    sub_buf: Vec<u8>,
    pub naws_enabled: bool,
    pub cols: u16,
    pub rows: u16,
    /// Recently received characters for login/username prompt detection (auto-login, once only)
    login_user: Option<String>,
    tail: Vec<u8>,
}

impl Parser {
    fn new(cols: u16, rows: u16, login_user: Option<String>) -> Self {
        Self {
            state: State::Normal,
            sub_buf: Vec::new(),
            naws_enabled: false,
            cols,
            rows,
            login_user: login_user.filter(|u| !u.is_empty()),
            tail: Vec::new(),
        }
    }

    /// Parse received bytes and return (application data, replies to send to the server)
    fn feed(&mut self, input: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut data = Vec::with_capacity(input.len());
        let mut reply = Vec::new();

        for &b in input {
            match self.state {
                State::Normal => {
                    if b == IAC {
                        self.state = State::Iac;
                    } else {
                        data.push(b);
                    }
                }
                State::Iac => match b {
                    IAC => {
                        data.push(IAC);
                        self.state = State::Normal;
                    }
                    WILL | WONT | DO | DONT => self.state = State::Negotiate(b),
                    SB => self.state = State::Sub,
                    _ => self.state = State::Normal, // ignore NOP, GA, etc.
                },
                State::Negotiate(cmd) => {
                    self.negotiate(cmd, b, &mut reply);
                    self.state = State::Normal;
                }
                State::Sub => {
                    self.sub_buf.clear();
                    self.state = State::SubData(b);
                }
                State::SubData(opt) => {
                    if b == IAC {
                        self.state = State::SubIac(opt);
                    } else {
                        self.sub_buf.push(b);
                    }
                }
                State::SubIac(opt) => match b {
                    SE => {
                        self.subnegotiate(opt, &mut reply);
                        self.state = State::Normal;
                    }
                    IAC => {
                        self.sub_buf.push(IAC);
                        self.state = State::SubData(opt);
                    }
                    _ => self.state = State::Normal,
                },
            }
        }

        self.auto_login(&data, &mut reply);
        (data, reply)
    }

    fn negotiate(&mut self, cmd: u8, opt: u8, reply: &mut Vec<u8>) {
        match cmd {
            WILL => {
                // Features the server offers: accept BINARY (8-bit), ECHO and SGA
                if opt == OPT_BINARY || opt == OPT_ECHO || opt == OPT_SGA {
                    reply.extend_from_slice(&[IAC, DO, opt]);
                } else {
                    reply.extend_from_slice(&[IAC, DONT, opt]);
                }
            }
            DO => {
                // Features the server requests from us
                match opt {
                    OPT_NAWS => {
                        reply.extend_from_slice(&[IAC, WILL, opt]);
                        self.naws_enabled = true;
                        reply.extend_from_slice(&naws_subneg(self.cols, self.rows));
                    }
                    OPT_BINARY | OPT_TTYPE | OPT_SGA => {
                        reply.extend_from_slice(&[IAC, WILL, opt]);
                    }
                    _ => reply.extend_from_slice(&[IAC, WONT, opt]),
                }
            }
            _ => {} // WONT/DONT need no reply
        }
    }

    fn subnegotiate(&mut self, opt: u8, reply: &mut Vec<u8>) {
        // TERMINAL-TYPE SEND → IS "xterm-256color"
        if opt == OPT_TTYPE && self.sub_buf.first() == Some(&TTYPE_SEND) {
            reply.extend_from_slice(&[IAC, SB, OPT_TTYPE, TTYPE_IS]);
            reply.extend_from_slice(TERM_NAME);
            reply.extend_from_slice(&[IAC, SE]);
        }
    }

    /// Auto-send the configured user ID once when a "login:" / "username:" prompt appears
    fn auto_login(&mut self, data: &[u8], reply: &mut Vec<u8>) {
        let Some(user) = &self.login_user else {
            return;
        };
        if data.is_empty() {
            return;
        }
        self.tail.extend(
            data.iter()
                .filter(|b| b.is_ascii() && !b"\r\n".contains(b))
                .map(|b| b.to_ascii_lowercase()),
        );
        if self.tail.len() > 64 {
            let cut = self.tail.len() - 64;
            self.tail.drain(..cut);
        }
        let tail = String::from_utf8_lossy(&self.tail);
        let trimmed = tail.trim_end();
        if trimmed.ends_with("login:") || trimmed.ends_with("username:") {
            reply.extend_from_slice(user.as_bytes());
            reply.extend_from_slice(b"\r\n");
            self.login_user = None;
        }
    }
}
