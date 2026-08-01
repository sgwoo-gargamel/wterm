import { invoke } from '@tauri-apps/api/core';
import type { Profile } from './ipc';

/** Everything wterm remembers, stored as one JSON document */
export interface PersistedSettings {
	locale?: 'ko' | 'en';
	theme?: 'dark' | 'light';
	themeColors?: unknown;
	font?: unknown;
	logDir?: string;
	profiles?: unknown[];
	last_by_type?: unknown;
	last_type?: Profile['type'] | null;
}

let data: PersistedSettings = {};
let saveTimer: ReturnType<typeof setTimeout> | undefined;

export async function initPersist() {
	try {
		const text = await invoke<string>('load_settings');
		data = text ? (JSON.parse(text) as PersistedSettings) : {};
	} catch {
		data = {};
	}
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
		void invoke('save_settings', { json: JSON.stringify(data, null, 2) }).catch(() => {});
	}, 200);
}

/** Absolute path of the settings file (next to the executable) */
export function settingsLocation(): Promise<string> {
	return invoke<string>('settings_location');
}
