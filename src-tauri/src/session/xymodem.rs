use std::collections::VecDeque;
use std::time::Duration;

use serial2_tokio::SerialPort;
use tauri::ipc::Channel;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{OutputEvent, SessionInput};

/// Which flavour of the XMODEM family to speak
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Protocol {
    /// Bare blocks only: 1K blocks (XMODEM-1K) when the receiver asks for
    /// CRC, 128-byte blocks for an old checksum-only receiver
    Xmodem,
    /// Batch mode: file-name header block, 1K data blocks, empty terminator
    Ymodem,
}

const SOH: u8 = 0x01; // 128-byte block
const STX: u8 = 0x02; // 1024-byte block
const EOT: u8 = 0x04;
const ACK: u8 = 0x06;
const NAK: u8 = 0x15;
const CAN: u8 = 0x18;
const CRC: u8 = b'C'; // receiver requests CRC-16 mode
const SUB: u8 = 0x1a; // padding for the last partial block

/// The user is expected to have started the receiver (rx/rb, loadx/loady)
/// already, but give them room to notice the overlay and cancel if they forgot.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60);
/// Per-block reply timeout. Receivers answer within a second normally; a slow
/// flash write on the target is the only legitimate reason to wait longer.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
const RETRIES: u32 = 10;
/// Standard ZMODEM/YMODEM abort: CANs to stop the receiver, BSs to erase them
/// from the line in case it already dropped back to a shell prompt.
const ABORT: [u8; 10] = [CAN, CAN, CAN, CAN, CAN, 0x08, 0x08, 0x08, 0x08, 0x08];

/// Receiver poll byte: 'C' asks for CRC-16 blocks, NAK for 8-bit checksum
pub fn is_poll(b: u8) -> bool {
    b == CRC || b == NAK
}

/// What the serial session loop should do once the transfer attempt is over
pub enum After {
    /// Keep the session running (success, failure and cancel are already
    /// reported to the frontend as transfer events)
    Continue,
    /// The session itself is over (port error, or Close arrived mid-transfer)
    End(String),
}

enum Fail {
    /// User pressed cancel in the UI
    Cancelled,
    /// Receiver aborted with CAN CAN
    RemoteCancel,
    /// Stable token for the frontend ("handshake-timeout" / "transfer-timeout")
    Timeout(&'static str),
    /// Port I/O failed — the session is dead
    Port(String),
    /// SessionInput::Close arrived — the session is being torn down
    Closed,
}

/// Send `path` over the serial port as a plain XMODEM file or a single-file
/// YMODEM batch. Runs inline in the session task so protocol bytes never
/// reach the terminal. `poll` is a receiver poll byte the session loop
/// already saw (held back during the file dialog); it satisfies the opening
/// handshake so the transfer needs no wait for the receiver's next retry.
pub async fn send(
    port: &SerialPort,
    rx: &mut mpsc::Receiver<SessionInput>,
    output: &Channel<OutputEvent>,
    path: &str,
    protocol: Protocol,
    poll: Option<u8>,
) -> After {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let data = match tokio::fs::read(path).await {
        Ok(data) => data,
        Err(e) => {
            let _ = output.send(OutputEvent::TransferFailed {
                reason: e.to_string(),
            });
            return After::Continue;
        }
    };

    let _ = output.send(OutputEvent::TransferStart {
        name: name.clone(),
        size: data.len() as u64,
    });

    let mut x = Xfer {
        port,
        rx,
        output,
        pending: poll.into_iter().collect(),
    };
    match run(&mut x, protocol, &name, &data).await {
        Ok(()) => {
            let _ = output.send(OutputEvent::TransferDone { name });
            After::Continue
        }
        Err(Fail::Port(reason)) => After::End(reason),
        Err(Fail::Closed) => After::End("closed".to_string()),
        Err(fail) => {
            // Stop the receiver so it does not sit waiting for the next block
            let _ = port.write_all(&ABORT).await;
            let reason = match fail {
                Fail::Cancelled => "cancelled".to_string(),
                Fail::RemoteCancel => "remote-cancelled".to_string(),
                Fail::Timeout(token) => token.to_string(),
                Fail::Port(_) | Fail::Closed => unreachable!(),
            };
            let _ = output.send(OutputEvent::TransferFailed { reason });
            After::Continue
        }
    }
}

async fn run(x: &mut Xfer<'_>, protocol: Protocol, name: &str, data: &[u8]) -> Result<(), Fail> {
    // The receiver polls its mode byte until the sender shows up
    let crc = x.wait_handshake(HANDSHAKE_TIMEOUT).await?;
    let _ = x.output.send(OutputEvent::TransferHandshake);

    if protocol == Protocol::Ymodem {
        // Block 0: file name NUL decimal-size, zero-padded (rb also appends
        // mtime and mode, but every receiver treats those as optional)
        let mut meta = Vec::with_capacity(128);
        meta.extend_from_slice(name.as_bytes());
        meta.push(0);
        meta.extend_from_slice(data.len().to_string().as_bytes());
        meta.resize(if meta.len() <= 128 { 128 } else { 1024 }, 0);
        x.send_block_ack(0, &meta, crc).await?;

        // The receiver re-arms with another mode byte before the data phase
        x.wait_handshake(Duration::from_secs(15)).await?;
    }

    // XMODEM-1K is tied to CRC mode; a receiver that only knows the 8-bit
    // checksum predates 1K blocks and would reject STX
    let block = if protocol == Protocol::Ymodem || crc { 1024 } else { 128 };

    let mut sent = 0u64;
    let mut blk: u8 = 1;
    for chunk in data.chunks(block) {
        if chunk.len() == block {
            x.send_block_ack(blk, chunk, crc).await?;
        } else {
            let mut padded = vec![SUB; if chunk.len() <= 128 { 128 } else { 1024 }];
            padded[..chunk.len()].copy_from_slice(chunk);
            x.send_block_ack(blk, &padded, crc).await?;
        }
        sent += chunk.len() as u64;
        blk = blk.wrapping_add(1);
        let _ = x.output.send(OutputEvent::TransferProgress {
            sent,
            size: data.len() as u64,
        });
    }

    x.send_eot().await?;

    if protocol == Protocol::Ymodem {
        // End the batch: the receiver asks for the next file, an all-zero
        // header answers "none". Every data block is already acknowledged by
        // now, so a receiver that skips this phase (or already gave up) still
        // got the file — only real failures (port death, session close,
        // cancel) propagate.
        match x.wait_handshake(REPLY_TIMEOUT).await {
            Ok(crc) => match x.send_block_ack(0, &[0u8; 128], crc).await {
                Ok(()) | Err(Fail::Timeout(_)) => {}
                Err(e) => return Err(e),
            },
            Err(Fail::Timeout(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

struct Xfer<'a> {
    port: &'a SerialPort,
    rx: &'a mut mpsc::Receiver<SessionInput>,
    output: &'a Channel<OutputEvent>,
    /// Bytes read from the port but not yet consumed by the protocol
    pending: VecDeque<u8>,
}

impl Xfer<'_> {
    /// Next byte from the receiver, or None on timeout. Session input is
    /// serviced while waiting: cancel/close abort, keystrokes are dropped
    /// (they would corrupt the protocol stream).
    async fn read_byte(&mut self, dur: Duration) -> Result<Option<u8>, Fail> {
        if let Some(b) = self.pending.pop_front() {
            return Ok(Some(b));
        }
        let deadline = Instant::now() + dur;
        let mut buf = [0u8; 256];
        loop {
            tokio::select! {
                read = self.port.read(&mut buf) => match read {
                    // Same as the session loop: 0 bytes without a timeout set
                    // means the device is gone
                    Ok(0) => return Err(Fail::Port("device-removed".to_string())),
                    Ok(n) => {
                        self.pending.extend(&buf[..n]);
                        return Ok(self.pending.pop_front());
                    }
                    Err(e) => return Err(Fail::Port(e.to_string())),
                },
                input = self.rx.recv() => match input {
                    Some(SessionInput::TransferCancel) => return Err(Fail::Cancelled),
                    Some(SessionInput::Close) | None => return Err(Fail::Closed),
                    Some(_) => {}
                },
                _ = tokio::time::sleep_until(deadline) => return Ok(None),
            }
        }
    }

    async fn write(&self, bytes: &[u8]) -> Result<(), Fail> {
        self.port
            .write_all(bytes)
            .await
            .map_err(|e| Fail::Port(e.to_string()))
    }

    /// Wait for the receiver's mode byte: 'C' → CRC-16, NAK → 8-bit checksum.
    /// Anything else on the line (boot noise, a stale prompt) is skipped.
    async fn wait_handshake(&mut self, dur: Duration) -> Result<bool, Fail> {
        let deadline = Instant::now() + dur;
        let mut cancels = 0u32;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Fail::Timeout("handshake-timeout"));
            }
            match self.read_byte(left).await? {
                Some(CRC) => return Ok(true),
                Some(NAK) => return Ok(false),
                Some(CAN) => {
                    cancels += 1;
                    if cancels >= 2 {
                        return Err(Fail::RemoteCancel);
                    }
                }
                Some(_) => cancels = 0,
                None => return Err(Fail::Timeout("handshake-timeout")),
            }
        }
    }

    /// Send one block and repeat until the receiver ACKs it
    async fn send_block_ack(&mut self, blk: u8, payload: &[u8], crc: bool) -> Result<(), Fail> {
        let mut frame = Vec::with_capacity(payload.len() + 5);
        frame.push(if payload.len() == 1024 { STX } else { SOH });
        frame.push(blk);
        frame.push(!blk);
        frame.extend_from_slice(payload);
        if crc {
            let c = crc16(payload);
            frame.push((c >> 8) as u8);
            frame.push(c as u8);
        } else {
            frame.push(payload.iter().fold(0u8, |a, &b| a.wrapping_add(b)));
        }

        let mut cancels = 0u32;
        for _ in 0..RETRIES {
            self.write(&frame).await?;
            loop {
                match self.read_byte(REPLY_TIMEOUT).await? {
                    Some(ACK) => return Ok(()),
                    Some(NAK) => break, // corrupted on the wire — resend
                    Some(CAN) => {
                        cancels += 1;
                        if cancels >= 2 {
                            return Err(Fail::RemoteCancel);
                        }
                    }
                    // A handshake byte still in flight from before this block;
                    // keep waiting for the verdict on the block itself
                    Some(_) => cancels = 0,
                    None => break, // reply lost — resend
                }
            }
        }
        Err(Fail::Timeout("transfer-timeout"))
    }

    /// Send EOT until acknowledged (receivers NAK the first one by design)
    async fn send_eot(&mut self) -> Result<(), Fail> {
        let mut cancels = 0u32;
        for _ in 0..RETRIES {
            self.write(&[EOT]).await?;
            match self.read_byte(REPLY_TIMEOUT).await? {
                Some(ACK) => return Ok(()),
                Some(CAN) => {
                    cancels += 1;
                    if cancels >= 2 {
                        return Err(Fail::RemoteCancel);
                    }
                }
                // The receiver ACKed and moved on to asking for the next file;
                // hand the byte back for the terminate phase
                Some(CRC) => {
                    self.pending.push_front(CRC);
                    return Ok(());
                }
                Some(_) | None => {} // NAK or lost — resend
            }
        }
        Err(Fail::Timeout("transfer-timeout"))
    }
}

/// CRC-16/XMODEM (poly 0x1021, init 0)
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
