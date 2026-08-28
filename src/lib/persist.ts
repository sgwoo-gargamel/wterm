import { invoke } from '@tauri-apps/api/core';
import type { Profile } from './ipc';
import { reportSaveFailure, clearSaveFailure } from './stores/savestatus.svelte';

/** Everything wterm remembers, stored as one JSON document */
export interface PersistedSettings {
	locale?: 'ko' | 'en';
	theme?: 'dark' | 'light';
	themeColors?: unknown;
	chromeColors?: unknown;
	font?: unknown;
	renderer?: 'webgl' | 'dom';
	logDir?: string;
	profiles?: unknown[];
	last_by_type?: unknown;
	last_type?: Profile['type'] | null;
	multi_history?: string[];
	workspaces?: unknown[];
}

let data: PersistedSettings = {};
let saveTimer: ReturnType<typeof setTimeout> | undefined;

async function writeSettings() {
	try {
		await invoke('save_settings', { json: JSON.stringify(data, null, 2) });
		clearSaveFailure();
	} catch (err) {
		const path = await settingsLocation().catch(() => '');
		reportSaveFailure(err instanceof Error ? err.message : String(err), path);
	}
}

export async function initPersist() {
	let text = '';
	try {
		text = await invoke<string>('load_settings');
	} catch {
		text = '';
	}

	let readable = true;
	try {
		data = text ? (JSON.parse(text) as PersistedSettings) : {};
	} catch {
		data = {};
		readable = false;
	}

	// Write once at startup so the file exists from the first run and a read-only
	// location is reported before the user does any work worth losing. Skipped when
	// the existing file is unparseable, since writing would destroy its contents.
	if (readable) await writeSettings();
}

export function getSetting<K extends keyof PersistedSettings>(
	key: K
): PersistedSettings[K] | undefined {
	return data[key];
}

/** Update one field; writes are coalesced so rapid changes cost one file write */
export function setSetting<K extends keyof PersistedSettings>(
	key: K,
	value: PersistedSettings[K]
) {
	data[key] = value;
	clearTimeout(saveTimer);
	saveTimer = setTimeout(() => {
		void writeSettings();
	}, 200);
}

/** Absolute path of the settings file (next to the executable) */
export function settingsLocation(): Promise<string> {
	return invoke<string>('settings_location');
}
