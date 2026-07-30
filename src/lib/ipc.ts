import { invoke, Channel } from '@tauri-apps/api/core';

// Maps 1:1 to the Rust Profile enum (serde tag = "type")
export type SerialProfile = { type: 'serial'; port: string; baud_rate: number };
export type SshProfile = { type: 'ssh'; host: string; port: number; username: string };
export type TelnetProfile = { type: 'telnet'; host: string; port: number; username?: string | null };
export type LocalProfile = { type: 'local'; command: string; cwd?: string | null };
export type Profile = SerialProfile | SshProfile | TelnetProfile | LocalProfile;

export interface ShellInfo {
	label: string;
	command: string;
}

/** Installed WSL distributions plus standard shells */
export function listShells(): Promise<ShellInfo[]> {
	return invoke<ShellInfo[]>('list_shells');
}

export type OutputEvent =
	| { type: 'data'; bytes: number[] }
	| { type: 'connected' }
	| { type: 'disconnected'; reason: string };

export interface PortInfo {
	name: string;
	kind: string;
	in_use: boolean;
}

export function listSerialPorts(): Promise<PortInfo[]> {
	return invoke<PortInfo[]>('list_serial_ports');
}

/** Installed fixed-pitch (monospace) font family names */
export function listFonts(): Promise<string[]> {
	return invoke<string[]>('list_fonts');
}

export function openSession(
	profile: Profile,
	password: string | null,
	cols: number,
	rows: number,
	onEvent: (ev: OutputEvent) => void
): Promise<string> {
	const onOutput = new Channel<OutputEvent>();
	onOutput.onmessage = onEvent;
	return invoke<string>('session_open', { profile, password, cols, rows, onOutput });
}

export function writeSession(id: string, data: Uint8Array): Promise<void> {
	return invoke('session_write', { id, data: Array.from(data) });
}

export function resizeSession(id: string, cols: number, rows: number): Promise<void> {
	return invoke('session_resize', { id, cols, rows });
}

export function closeSession(id: string): Promise<void> {
	return invoke('session_close', { id });
}

export function startLog(
	id: string,
	path: string,
	timestamps: boolean,
	plain: boolean
): Promise<void> {
	return invoke('session_start_log', { id, path, timestamps, plain });
}

export function stopLog(id: string): Promise<void> {
	return invoke('session_stop_log', { id });
}

/** Windows-safe file name fragment */
function safe(part: string): string {
	return part.replace(/[\\/:*?"<>|@\s]+/g, '-').replace(/^-+|-+$/g, '');
}

/** `{type}-{target}` — the stable part of a log file name */
export function logBaseName(profile: Profile): string {
	switch (profile.type) {
		case 'serial':
			return safe(`serial-${profile.port}-${profile.baud_rate}`);
		case 'ssh':
			return safe(`ssh-${profile.username}-${profile.host}-${profile.port}`);
		case 'telnet':
			return safe(`telnet-${profile.host}-${profile.port}`);
		case 'local': {
			// `wsl.exe -d "Ubuntu-22.04"` → wsl-Ubuntu-22.04, `cmd.exe` → local-cmd
			const distro = /-d\s+"?([^"]+)"?/.exec(profile.command)?.[1];
			if (distro) return safe(`wsl-${distro}`);
			const program = profile.command.split(/\s+/)[0].replace(/\.exe$/i, '');
			return safe(`local-${program}`);
		}
	}
}

/** yyyymmdd-hhmmss.sss */
export function logTimestamp(d = new Date()): string {
	const p = (n: number, len = 2) => String(n).padStart(len, '0');
	return (
		`${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}` +
		`-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`
	);
}

/** Friendly name for a local shell command line */
function shellName(command: string): string {
	const distro = /-d\s+"?([^"]+)"?/.exec(command)?.[1];
	if (distro) return `WSL · ${distro}`;
	if (/^powershell/i.test(command)) return 'PowerShell';
	if (/^cmd/i.test(command)) return 'Command Prompt';
	return command;
}

/** Drive (or `~`) plus the final folder: `D:\work\tauri\wterm` → `D:\…\wterm` */
export function shortenPath(path: string): string {
	const sep = path.includes('\\') ? '\\' : '/';
	const home = /^[a-z]:[\\/]Users[\\/][^\\/]+/i.exec(path)?.[0];
	const drive = /^[a-z]:/i.exec(path)?.[0] ?? '';
	const prefix = home ? '~' : drive;
	const parts = path
		.slice((home ?? drive).length)
		.split(/[\\/]/)
		.filter(Boolean);
	if (parts.length === 0) return prefix || path;
	const ellipsis = parts.length > 1 ? `…${sep}` : '';
	return `${prefix}${sep}${ellipsis}${parts[parts.length - 1]}`;
}

function localTitle(profile: LocalProfile): string {
	const name = shellName(profile.command);
	const cwd = profile.cwd?.trim();
	return cwd ? `${name} ${shortenPath(cwd)}` : name;
}

export function profileTitle(profile: Profile): string {
	switch (profile.type) {
		case 'serial':
			return `${profile.port} · ${profile.baud_rate}`;
		case 'ssh':
			return `${profile.username}@${profile.host}:${profile.port}`;
		case 'telnet':
			return `telnet ${profile.host}:${profile.port}`;
		case 'local':
			return localTitle(profile);
	}
}
