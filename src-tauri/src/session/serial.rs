use serial2_tokio::SerialPort;
use tauri::ipc::Channel;
use tokio::sync::mpsc;

use super::{OutputEvent, SessionCleanup, SessionInput, SessionLogger};
use crate::error::Result;

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
        let reason = loop {
            tokio::select! {
                read = port.read(&mut buf) => match read {
                    // No read timeout is set, so 0 bytes is EOF, not "no data":
                    // the wrapper surfaces a surprise-removed device (USB
                    // unplug) as a broken pipe. Treating it as "no data" would
                    // spin on the dead port forever.
                    Ok(0) => break "device-removed".to_string(),
                    Ok(n) => {
                        logger.write(&buf[..n]);
                        let _ = output.send(OutputEvent::Data {
                            bytes: buf[..n].to_vec(),
                        });
                    }
                    Err(e) => break e.to_string(),
                },
                input = rx.recv() => match input {
                    Some(SessionInput::Data(data)) => {
                        if let Err(e) = port.write_all(&data).await {
                            break e.to_string();
                        }
                    }
                    Some(SessionInput::Resize { .. }) => {} // not applicable to serial
                    Some(SessionInput::Close) | None => break "closed".to_string(),
                },
            }
        };
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
