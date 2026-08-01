use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tauri::ipc::Channel;
use tokio::sync::mpsc;

use super::{OutputEvent, SessionCleanup, SessionInput, SessionLogger};
use crate::error::{Error, Result};

/// Split a command line into program + args, honouring double quotes
fn split_command(command: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in command.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Local shell session (WSL, PowerShell, cmd, …) running on a ConPTY.
/// portable-pty is blocking, so reads and writes get their own threads.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    command: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    mut rx: mpsc::Receiver<SessionInput>,
    output: Channel<OutputEvent>,
    cleanup: SessionCleanup,
    logger: SessionLogger,
) -> Result<()> {
    let parts = split_command(&command);
    let (program, args) = parts
        .split_first()
        .ok_or_else(|| Error::Pty("empty command".into()))?;

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| Error::Pty(e.to_string()))?;

    let mut cmd = CommandBuilder::new(program);
    cmd.args(args);
    cmd.env("TERM", "xterm-256color");
    // Explicit directory wins. Otherwise Windows shells start in the user's home,
    // while WSL is left alone so it opens in the distro's own home directory
    match cwd.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // A bad directory would otherwise surface as a cryptic CreateProcess error
        Some(dir) if !std::path::Path::new(dir).is_dir() => {
            return Err(Error::InvalidCwd(dir.into()));
        }
        Some(dir) => cmd.cwd(dir),
        None if !program.to_lowercase().contains("wsl") => {
            let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
            if let Some(home) = std::env::var_os(home_var) {
                cmd.cwd(home);
            }
        }
        None => {}
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| Error::Pty(e.to_string()))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| Error::Pty(e.to_string()))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| Error::Pty(e.to_string()))?;
    // The master must outlive the threads that use it (resize, EOF handling).
    // Held as an Option so it can be dropped once the shell exits.
    let master = Arc::new(Mutex::new(Some(pair.master)));
    let mut killer = child.clone_killer();

    let _ = output.send(OutputEvent::Connected);

    // Waiter thread: a ConPTY reader does not see EOF just because the shell
    // exited, so watch the child and close the master to unblock the reader
    {
        let master = Arc::clone(&master);
        std::thread::spawn(move || {
            let _ = child.wait();
            *master.lock().unwrap() = None;
        });
    }

    // Read thread: pump PTY output until the shell exits
    {
        let output = output.clone();
        let logger = logger.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        logger.write(&buf[..n]);
                        let _ = output.send(OutputEvent::Data {
                            bytes: buf[..n].to_vec(),
                        });
                    }
                }
            }
            let _ = output.send(OutputEvent::Disconnected {
                reason: "remote-closed".into(),
            });
        });
    }

    // Write thread: consume the session input channel
    std::thread::spawn(move || {
        while let Some(input) = rx.blocking_recv() {
            match input {
                SessionInput::Data(data) => {
                    if writer.write_all(&data).is_err() || writer.flush().is_err() {
                        break;
                    }
                }
                SessionInput::Resize { cols, rows } => {
                    if let Some(master) = master.lock().unwrap().as_ref() {
                        let _ = master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                }
                SessionInput::Close => break,
            }
        }
        let _ = killer.kill();
        logger.stop();
        let _ = output.send(OutputEvent::Disconnected {
            reason: "closed".into(),
        });
        cleanup.run();
    });

    Ok(())
}
