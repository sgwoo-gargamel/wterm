use std::path::PathBuf;
use std::sync::Arc;

use russh::client::{self, Config, Handle, Handler};
use russh::keys::{load_secret_key, Algorithm, PrivateKeyWithHashAlg, PublicKey};
use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf};
use tauri::ipc::Channel as IpcChannel;
use tokio::sync::mpsc;

use super::{OutputEvent, SessionCleanup, SessionInput, SessionLogger};
use crate::error::{Error, Result};

struct ClientHandler;

impl Handler for ClientHandler {
    type Error = russh::Error;

    // TODO: extend with known_hosts verification / user confirmation dialog
    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Default key files tried in order, like the OpenSSH client does
fn default_key_paths() -> Vec<PathBuf> {
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let Some(home) = std::env::var_os(home) else {
        return Vec::new();
    };
    let ssh_dir = PathBuf::from(home).join(".ssh");
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|name| ssh_dir.join(name))
        .filter(|p| p.exists())
        .collect()
}

/// Try public-key auth with every default unencrypted key. Returns true on success.
async fn try_key_auth(handle: &mut Handle<ClientHandler>, username: &str) -> bool {
    for path in default_key_paths() {
        // Passphrase-protected keys are skipped (would need an extra prompt)
        let Ok(key) = load_secret_key(&path, None) else {
            continue;
        };
        let hash_alg = if matches!(key.algorithm(), Algorithm::Rsa { .. }) {
            match handle.best_supported_rsa_hash().await {
                Ok(h) => h.flatten(),
                Err(_) => None,
            }
        } else {
            None
        };
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);
        if let Ok(res) = handle.authenticate_publickey(username, key).await {
            if res.success() {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn(
    host: String,
    port: u16,
    username: String,
    password: String,
    cols: u16,
    rows: u16,
    rx: mpsc::Receiver<SessionInput>,
    output: IpcChannel<OutputEvent>,
    cleanup: SessionCleanup,
    logger: SessionLogger,
) -> Result<()> {
    let config = Arc::new(Config::default());
    let mut handle = client::connect(config, (host.as_str(), port), ClientHandler).await?;

    // Auth order: "none" (passwordless accounts), then keys (~/.ssh/id_*),
    // then the password from the form
    let mut authenticated = handle
        .authenticate_none(&username)
        .await
        .map(|r| r.success())
        .unwrap_or(false);

    if !authenticated {
        authenticated = try_key_auth(&mut handle, &username).await;
    }

    if !authenticated {
        if password.is_empty() {
            return Err(Error::PasswordRequired);
        }
        let auth = handle.authenticate_password(username, password).await?;
        if !auth.success() {
            return Err(Error::AuthFailed);
        }
    }

    let channel = handle.channel_open_session().await?;
    channel
        .request_pty(false, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
        .await?;
    channel.request_shell(false).await?;

    let _ = output.send(OutputEvent::Connected);
    let (read_half, write_half) = channel.split();
    tauri::async_runtime::spawn(run(
        handle, read_half, write_half, rx, output, cleanup, logger,
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run(
    handle: client::Handle<ClientHandler>,
    mut read_half: ChannelReadHalf,
    write_half: ChannelWriteHalf<client::Msg>,
    mut rx: mpsc::Receiver<SessionInput>,
    output: IpcChannel<OutputEvent>,
    cleanup: SessionCleanup,
    logger: SessionLogger,
) {
    let reason = loop {
        tokio::select! {
            msg = read_half.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        logger.write(&data);
                        let _ = output.send(OutputEvent::Data { bytes: data.to_vec() });
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        logger.write(&data);
                        let _ = output.send(OutputEvent::Data { bytes: data.to_vec() });
                    }
                    Some(ChannelMsg::Close) | Some(ChannelMsg::Eof) | None => {
                        break "remote-closed".to_string();
                    }
                    _ => {} // ExitStatus, Success, etc.
                }
            }
            input = rx.recv() => {
                match input {
                    Some(SessionInput::Data(data)) => {
                        if write_half.data(&data[..]).await.is_err() {
                            break "write-failed".to_string();
                        }
                    }
                    Some(SessionInput::Resize { cols, rows }) => {
                        let _ = write_half.window_change(cols as u32, rows as u32, 0, 0).await;
                    }
                    Some(SessionInput::Close) | None => {
                        let _ = write_half.close().await;
                        break "closed".to_string();
                    }
                }
            }
        }
    };

    logger.stop();
    let _ = output.send(OutputEvent::Disconnected { reason });
    cleanup.run();
    drop(handle);
}
