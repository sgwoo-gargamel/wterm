use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::error::Result;

/// Escape-sequence parser state for plain-text mode
enum EscState {
    Normal,
    Esc,
    /// Inside CSI (ESC [) — collecting parameter bytes
    Csi(Vec<u8>),
    /// Inside OSC (ESC ]) — runs until BEL or ST
    Osc,
    /// Saw ESC while in OSC (waiting for the backslash of ST)
    OscEsc,
}

struct Sink {
    writer: BufWriter<File>,
    /// Prefix every line with the local time it was received
    timestamps: bool,
    /// Reconstruct the visible text instead of writing raw bytes
    plain: bool,

    // --- raw mode ---
    at_line_start: bool,

    // --- plain mode: one rendered line, rewritten as the shell edits it ---
    state: EscState,
    line: Vec<u8>,
    cursor: usize,
    line_stamp: Option<String>,
}

fn now_stamp() -> String {
    chrono::Local::now()
        .format("[%Y-%m-%d %H:%M:%S%.3f] ")
        .to_string()
}

impl Sink {
    /// Write the rendered line out and start a new one
    fn flush_line(&mut self, newline: bool) {
        if self.line.is_empty() && self.line_stamp.is_none() {
            if newline {
                let _ = self.writer.write_all(b"\n");
            }
            return;
        }
        if self.timestamps {
            if let Some(stamp) = &self.line_stamp {
                let _ = self.writer.write_all(stamp.as_bytes());
            }
        }
        let _ = self.writer.write_all(&self.line);
        if newline {
            let _ = self.writer.write_all(b"\n");
        }
        self.line.clear();
        self.cursor = 0;
        self.line_stamp = None;
    }

    fn put_char(&mut self, byte: u8) {
        if self.line_stamp.is_none() {
            self.line_stamp = Some(now_stamp());
        }
        if self.cursor < self.line.len() {
            self.line[self.cursor] = byte;
        } else {
            self.line.push(byte);
        }
        self.cursor += 1;
    }

    /// Apply the CSI sequences that change visible text; ignore the rest (colors, …)
    fn apply_csi(&mut self, params: &[u8], final_byte: u8) {
        let text = String::from_utf8_lossy(params);
        let n = text
            .split(';')
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1);
        match final_byte {
            b'P' => {
                // DCH: delete n chars at the cursor
                let end = (self.cursor + n).min(self.line.len());
                if self.cursor < self.line.len() {
                    self.line.drain(self.cursor..end);
                }
            }
            b'@' => {
                // ICH: insert n blanks at the cursor
                let at = self.cursor.min(self.line.len());
                for _ in 0..n {
                    self.line.insert(at, b' ');
                }
            }
            b'C' => self.cursor = (self.cursor + n).min(self.line.len()),
            b'D' => self.cursor = self.cursor.saturating_sub(n),
            b'G' => self.cursor = (n - 1).min(self.line.len()),
            b'K' => {
                // EL: 0/none = to end of line, 1 = to start, 2 = whole line
                match text.trim().parse::<u32>().unwrap_or(0) {
                    1 => {
                        for i in 0..self.cursor.min(self.line.len()) {
                            self.line[i] = b' ';
                        }
                    }
                    2 => self.line.clear(),
                    _ => self.line.truncate(self.cursor.min(self.line.len())),
                }
            }
            _ => {}
        }
    }

    fn feed_plain(&mut self, data: &[u8]) {
        for &byte in data {
            match self.state {
                EscState::Normal => match byte {
                    0x1b => self.state = EscState::Esc,
                    b'\n' => self.flush_line(true),
                    b'\r' => self.cursor = 0,
                    0x08 => self.cursor = self.cursor.saturating_sub(1),
                    0x07 => {}                          // BEL
                    b'\t' => self.put_char(b'\t'),
                    b if b >= 0x20 || b >= 0x80 => self.put_char(b),
                    _ => {}                             // other control chars
                },
                EscState::Esc => match byte {
                    b'[' => self.state = EscState::Csi(Vec::new()),
                    b']' => self.state = EscState::Osc,
                    // Two-byte sequences (ESC =, ESC >, …) end here
                    _ => self.state = EscState::Normal,
                },
                EscState::Csi(ref mut params) => {
                    if (0x30..=0x3f).contains(&byte) {
                        params.push(byte);
                    } else if (0x20..=0x2f).contains(&byte) {
                        // intermediate bytes — ignored
                    } else {
                        let params = std::mem::take(params);
                        self.state = EscState::Normal;
                        self.apply_csi(&params, byte);
                    }
                }
                EscState::Osc => match byte {
                    0x07 => self.state = EscState::Normal,
                    0x1b => self.state = EscState::OscEsc,
                    _ => {}
                },
                EscState::OscEsc => self.state = EscState::Normal,
            }
        }
        // Always flush: completed lines are in the writer's buffer and would
        // otherwise sit there, leaving the file empty until the buffer fills
        let _ = self.writer.flush();
    }

    fn feed_raw(&mut self, data: &[u8]) {
        if !self.timestamps {
            let _ = self.writer.write_all(data);
            return;
        }
        let stamp = now_stamp();
        for &byte in data {
            if self.at_line_start && byte != b'\r' && byte != b'\n' {
                let _ = self.writer.write_all(stamp.as_bytes());
                self.at_line_start = false;
            }
            let _ = self.writer.write_all(&[byte]);
            if byte == b'\n' {
                self.at_line_start = true;
            }
        }
    }
}

/// Optional file sink for a session's output, toggled at runtime from the UI.
/// Cloning shares the same sink, so the manager and the session task agree.
#[derive(Clone, Default)]
pub struct SessionLogger {
    inner: Arc<Mutex<Option<Sink>>>,
}

impl SessionLogger {
    pub fn start(&self, path: &str, timestamps: bool, plain: bool) -> Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        *self.inner.lock().unwrap() = Some(Sink {
            writer: BufWriter::new(file),
            timestamps,
            plain,
            at_line_start: true,
            state: EscState::Normal,
            line: Vec::new(),
            cursor: 0,
            line_stamp: None,
        });
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(mut sink) = self.inner.lock().unwrap().take() {
            if sink.plain {
                sink.flush_line(true);
            }
            let _ = sink.writer.flush();
        }
    }

    /// Append received bytes; flushed eagerly so the file stays readable live
    pub fn write(&self, data: &[u8]) {
        let mut guard = self.inner.lock().unwrap();
        let Some(sink) = guard.as_mut() else {
            return;
        };
        if sink.plain {
            sink.feed_plain(data);
        } else {
            sink.feed_raw(data);
            let _ = sink.writer.flush();
        }
    }
}
