use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::error::Result;
use crate::session::{OutputEvent, Profile, SessionInput, SessionManager};

#[derive(Serialize)]
pub struct PortInfo {
    pub name: String,
    pub kind: String,
    pub in_use: bool,
}

/// `(async)` runs this on the thread pool. Enumeration probe-opens every port,
/// which a busy driver can stall on — on the main thread that would hold up the
/// whole IPC queue, leaving an in-flight `session_open` hanging forever.
#[tauri::command(async)]
pub fn list_serial_ports() -> Result<Vec<PortInfo>> {
    let ports = serialport::available_ports()?;
    Ok(ports
        .into_iter()
        .map(|p| {
            let bluetooth = matches!(&p.port_type, serialport::SerialPortType::BluetoothPort);
            let kind = match p.port_type {
                serialport::SerialPortType::UsbPort(info) => {
                    info.product.unwrap_or_else(|| "USB".into())
                }
                serialport::SerialPortType::BluetoothPort => "Bluetooth".into(),
                serialport::SerialPortType::PciPort => "PCI".into(),
                serialport::SerialPortType::Unknown => String::new(),
            };
            // Probe-open to detect ports held by another app or one of our sessions.
            // Never for Bluetooth: opening an outgoing SPP port makes Windows try to
            // establish the link, which blocks for tens of seconds when the device is
            // away — and "busy" says nothing useful about a port that is not paired up
            // anyway. Those are reported free and left for the connect attempt to judge.
            let in_use = !bluetooth
                && serialport::new(&p.port_name, 9600)
                    .timeout(std::time::Duration::from_millis(50))
                    .open()
                    .is_err();
            PortInfo {
                name: p.port_name,
                kind,
                in_use,
            }
        })
        .collect())
}

/// Enumerate installed fixed-pitch (monospace) font family names via GDI.
/// Off the main thread — a full GDI enumeration is not instant.
#[cfg(windows)]
#[tauri::command(async)]
pub fn list_fonts() -> Vec<String> {
    use std::collections::BTreeSet;
    use windows_sys::Win32::Foundation::LPARAM;
    use windows_sys::Win32::Graphics::Gdi::{
        EnumFontFamiliesExW, GetDC, ReleaseDC, DEFAULT_CHARSET, LOGFONTW, TEXTMETRICW,
        TMPF_FIXED_PITCH,
    };

    unsafe extern "system" fn callback(
        lf: *const LOGFONTW,
        tm: *const TEXTMETRICW,
        _font_type: u32,
        lparam: LPARAM,
    ) -> i32 {
        let set = unsafe { &mut *(lparam as *mut BTreeSet<String>) };
        let (lf, tm) = unsafe { (&*lf, &*tm) };
        // Counter-intuitive GDI flag: the bit is SET for variable pitch fonts
        if tm.tmPitchAndFamily & TMPF_FIXED_PITCH != 0 {
            return 1;
        }
        let len = lf
            .lfFaceName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(lf.lfFaceName.len());
        let name = String::from_utf16_lossy(&lf.lfFaceName[..len]);
        // '@'-prefixed families are vertical-writing variants
        if !name.starts_with('@') {
            set.insert(name);
        }
        1
    }

    let mut set: BTreeSet<String> = BTreeSet::new();
    unsafe {
        let hdc = GetDC(std::ptr::null_mut());
        let mut lf: LOGFONTW = std::mem::zeroed();
        lf.lfCharSet = DEFAULT_CHARSET;
        EnumFontFamiliesExW(
            hdc,
            &lf,
            Some(callback),
            &mut set as *mut _ as LPARAM,
            0,
        );
        ReleaseDC(std::ptr::null_mut(), hdc);
    }
    set.into_iter().collect()
}

#[cfg(not(windows))]
#[tauri::command(async)]
pub fn list_fonts() -> Vec<String> {
    Vec::new()
}

#[derive(Serialize)]
pub struct ShellInfo {
    pub label: String,
    pub command: String,
}

/// Installed WSL distributions (default first) plus the standard Windows shells.
/// Off the main thread: `wsl -l -q` shells out, and it can take seconds while a
/// distro is already running — the main thread must stay free to pump IPC.
#[cfg(windows)]
#[tauri::command(async)]
pub fn list_shells() -> Vec<ShellInfo> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut shells = Vec::new();

    // `wsl -l -q` prints distro names as UTF-16LE, default distro first
    if let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["-l", "-q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        let utf16: Vec<u16> = out
            .stdout
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        for line in String::from_utf16_lossy(&utf16).lines() {
            let name = line.trim().trim_matches('\0').trim();
            if name.is_empty() {
                continue;
            }
            shells.push(ShellInfo {
                label: format!("WSL · {name}"),
                command: format!("wsl.exe -d \"{name}\""),
            });
        }
    }

    shells.push(ShellInfo {
        label: "PowerShell".into(),
        command: "powershell.exe -NoLogo".into(),
    });
    shells.push(ShellInfo {
        label: "Command Prompt".into(),
        command: "cmd.exe".into(),
    });
    shells
}

#[cfg(not(windows))]
#[tauri::command(async)]
pub fn list_shells() -> Vec<ShellInfo> {
    vec![ShellInfo {
        label: "Shell".into(),
        command: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
    }]
}

#[tauri::command]
pub async fn session_open(
    state: State<'_, SessionManager>,
    profile: Profile,
    password: Option<String>,
    cols: u16,
    rows: u16,
    on_output: Channel<OutputEvent>,
) -> Result<String> {
    state.open(profile, password, cols, rows, on_output).await
}

#[tauri::command]
pub async fn session_write(
    state: State<'_, SessionManager>,
    id: String,
    data: Vec<u8>,
) -> Result<()> {
    state.send(&id, SessionInput::Data(data)).await
}

#[tauri::command]
pub async fn session_resize(
    state: State<'_, SessionManager>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    state.send(&id, SessionInput::Resize { cols, rows }).await
}

#[tauri::command]
pub async fn session_close(state: State<'_, SessionManager>, id: String) -> Result<()> {
    state.close(&id).await
}

/// Start writing this session's output to `path` (created/truncated).
/// `timestamps` prefixes every logged line with its arrival time;
/// `plain` renders the visible text instead of writing raw escape sequences.
/// Off the main thread — the log directory may be a slow or network path.
#[tauri::command(async)]
pub fn session_start_log(
    state: State<'_, SessionManager>,
    id: String,
    path: String,
    timestamps: bool,
    plain: bool,
) -> Result<()> {
    state.start_log(&id, &path, timestamps, plain)
}

#[tauri::command]
pub fn session_stop_log(state: State<'_, SessionManager>, id: String) {
    state.stop_log(&id);
}
