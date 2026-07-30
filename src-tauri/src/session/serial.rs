use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::ipc::Channel;
use tokio::sync::mpsc;

use super::{OutputEvent, SessionCleanup, SessionInput, SessionLogger};
use crate::error::Result;

/// Serial session. serialport is a blocking API, so read/write each run on a dedicated thread.
pub fn spawn(
    port_name: String,
    baud_rate: u32,
    mut rx: mpsc::Receiver<SessionInput>,
    output: Channel<OutputEvent>,
    cleanup: SessionCleanup,
    logger: SessionLogger,
) -> Result<()> {
    let mut reader = serialport::new(&port_name, baud_rate)
        .timeout(Duration::from_millis(50))
        .open()?;
    let mut writer = reader.try_clone()?;

    let _ = output.send(OutputEvent::Connected);
    let stop = Arc::new(AtomicBool::new(false));

    // Read thread: poll with a 50ms timeout, checking the stop flag between reads
    {
        let stop = Arc::clone(&stop);
        let output = output.clone();
        let logger = logger.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => {
                        logger.write(&buf[..n]);
                        let _ = output.send(OutputEvent::Data {
                            bytes: buf[..n].to_vec(),
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => {
                        if !stop.load(Ordering::Relaxed) {
                            let _ = output.send(OutputEvent::Disconnected {
                                reason: e.to_string(),
                            });
                        }
                        break;
                    }
                }
            }
        });
    }

    // Write thread: consume the session input channel
    std::thread::spawn(move || {
        while let Some(input) = rx.blocking_recv() {
            match input {
                SessionInput::Data(data) => {
                    if writer.write_all(&data).is_err() {
                        break;
                    }
                }
                SessionInput::Resize { .. } => {} // not applicable to serial
                SessionInput::Close => break,
            }
        }
        stop.store(true, Ordering::Relaxed);
        logger.stop();
        let _ = output.send(OutputEvent::Disconnected {
            reason: "closed".into(),
        });
        cleanup.run();
    });

    Ok(())
}
