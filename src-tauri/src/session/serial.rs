use std::time::{Duration, Instant};

use serial2_tokio::SerialPort;
use tauri::ipc::Channel;
use tokio::sync::mpsc;

use super::{xymodem, OutputEvent, SessionCleanup, SessionInput, SessionLogger};
use crate::error::Result;

/// Most port input to hold back during the file dialog before letting the
/// backlog through to the terminal (see the read arm of the session loop)
const HOLD_LIMIT: usize = 256 * 1024;

/// How long a receiver poll byte that already went to the terminal still
/// counts as "the receiver is waiting". Receivers re-poll every 2 s (U-Boot)
/// to 10 s (lrzsz), so a live one refreshes this well inside the window.
const POLL_FRESH: Duration = Duration::from_secs(30);

/// The poll byte that ends `bytes`, if the stream ends in the receiver's
/// poll run (only `C`/NAK after the last real output)
fn tail_poll(bytes: &[u8]) -> Option<(u8, Instant)> {
    bytes
        .last()
        .copied()
        .filter(|&b| xymodem::is_poll(b))
        .map(|b| (b, Instant::now()))
}

/// Serial session on serial2-tokio. Its overlapped I/O on Windows lets reads
/// and writes overlap on one port, so keystrokes go out immediately instead of
/// queueing behind a blocked read (the old serialport sync-handle problem).
///
/// Single task, telnet-style select!: the crate's `read` is readiness-based
/// (readable().await + try_read), so cancelling it in select! loses no data —
/// and whichever way the session ends, this one task drops the port handle.
pub fn spawn(
    port_name: String,
    baud_rate: u32,
    mut rx: mpsc::Receiver<SessionInput>,
    output: Channel<OutputEvent>,
    cleanup: SessionCleanup,
    logger: SessionLogger,
) -> Result<()> {
    let port = SerialPort::open(&port_name, baud_rate)?;

    let _ = output.send(OutputEvent::Connected);

    tauri::async_runtime::spawn(async move {
        let mut buf = [0u8; 4096];
        // Port input held back while the user is in the file dialog
        // (TransferPrepare … TransferSend/TransferCancel). None = not holding.
        let mut held: Option<Vec<u8>> = None;
        // Poll byte that ended the output shown so far: a receiver that
        // started polling before the dialog opened, and whose next poll did
        // not arrive while it was open, is still waiting for the transfer
        let mut last_poll: Option<(u8, Instant)> = None;
        let emit = |bytes: &[u8]| {
            if bytes.is_empty() {
                return;
            }
            logger.write(bytes);
            let _ = output.send(OutputEvent::Data {
                bytes: bytes.to_vec(),
            });
        };
        let reason = loop {
            tokio::select! {
                read = port.read(&mut buf) => match read {
                    // No read timeout is set, so 0 bytes is EOF, not "no data":
                    // the wrapper surfaces a surprise-removed device (USB
                    // unplug) as a broken pipe. Treating it as "no data" would
                    // spin on the dead port forever.
                    Ok(0) => break "device-removed".to_string(),
                    Ok(n) => match held.as_mut() {
                        Some(held) => {
                            // A target that keeps logging while the dialog sits
                            // open must not pile up forever: let the backlog
                            // through and keep holding only the fresh tail
                            if held.len() + n > HOLD_LIMIT {
                                emit(held);
                                held.clear();
                            }
                            held.extend_from_slice(&buf[..n]);
                        }
                        None => {
                            emit(&buf[..n]);
                            last_poll = tail_poll(&buf[..n]);
                        }
                    },
                    Err(e) => break e.to_string(),
                },
                input = rx.recv() => match input {
                    Some(SessionInput::Data(data)) => {
                        if let Err(e) = port.write_all(&data).await {
                            break e.to_string();
                        }
                    }
                    Some(SessionInput::TransferPrepare) => {
                        held.get_or_insert_with(Vec::new);
                    }
                    // Runs the whole transfer inline: the protocol owns the
                    // port until it finishes, so its bytes never reach the
                    // terminal or the logger
                    Some(SessionInput::TransferSend { path, protocol }) => {
                        // The receiver's poll bytes that arrived during the
                        // dialog end the held input if it is still waiting;
                        // one of them stands in for its next poll so the
                        // transfer starts at once. Anything after the last
                        // poll byte means the receiver moved on (timed out,
                        // printed an error), so wait for a fresh poll instead.
                        let mut held = held.take().unwrap_or_default();
                        let tail = held
                            .iter()
                            .rposition(|&b| !xymodem::is_poll(b))
                            .map_or(0, |i| i + 1);
                        let poll = match held.split_off(tail).first().copied() {
                            Some(b) => Some(b),
                            // Dialog closed before the receiver polled again:
                            // the poll that reached the terminal before it
                            // opened still stands, as long as nothing came
                            // after it
                            None if held.is_empty() => last_poll
                                .filter(|(_, at)| at.elapsed() < POLL_FRESH)
                                .map(|(b, _)| b),
                            None => None,
                        };
                        emit(&held);
                        last_poll = None;
                        match xymodem::send(&port, &mut rx, &output, &path, protocol, poll).await {
                            xymodem::After::Continue => {}
                            xymodem::After::End(reason) => break reason,
                        }
                    }
                    // No transfer is running when it reaches this loop; this
                    // is the file dialog being dismissed
                    Some(SessionInput::TransferCancel) => {
                        if let Some(held) = held.take() {
                            emit(&held);
                            if !held.is_empty() {
                                last_poll = tail_poll(&held);
                            }
                        }
                    }
                    Some(SessionInput::Resize { .. }) => {} // not applicable to serial
                    Some(SessionInput::Close) | None => break "closed".to_string(),
                },
            }
        };
        // Whatever was held for a transfer that never started is real output
        if let Some(held) = held.take() {
            emit(&held);
        }
        // Release the COM handle before announcing the disconnect: a handle
        // held open on a surprise-removed USB adapter keeps the zombie device
        // instance alive, blocking re-enumeration when it is plugged back in.
        drop(port);
        logger.stop();
        let _ = output.send(OutputEvent::Disconnected { reason });
        cleanup.run();
    });

    Ok(())
}
