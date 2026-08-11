import type { LocalProfile, Profile, SerialProfile, SshProfile, TelnetProfile } from '$lib/ipc';
import { profileTitle } from '$lib/ipc';
import { getSetting, setSetting } from '$lib/persist';

export interface ProfileEntry {
	id: string;
	name: string;
	profile: Profile;
}

export interface LastByType {
	serial?: SerialProfile;
	ssh?: SshProfile;
	telnet?: TelnetProfile;
	local?: LocalProfile;
}

export const profilesState = $state<{ list: ProfileEntry[] }>({ list: [] });

/** Last-connected settings per transport type, used to prefill the connect form */
export const lastState = $state<{
	byType: LastByType;
	lastType: Profile['type'] | null;
	loaded: boolean;
}>({ byType: {}, lastType: null, loaded: false });

// Canonical key independent of JSON property order (the store round-trip alphabetizes keys)
function profileKey(p: Profile): string {
	switch (p.type) {
		case 'serial':
			return `serial|${p.port}|${p.baud_rate}`;
		case 'ssh':
			return `ssh|${p.host}|${p.port}|${p.username}`;
		case 'telnet':
			return `telnet|${p.host}|${p.port}|${p.username ?? ''}`;
		case 'local':
			return `local|${p.command}|${p.cwd ?? ''}`;
	}
}

export function loadProfiles() {
	const list = (getSetting('profiles') as ProfileEntry[] | undefined) ?? [];
	// Clean up legacy format (extra fields like lastUsed) and dedupe by content
	const seen = new Set<string>();
	const cleaned: ProfileEntry[] = [];
	for (const e of list) {
		if (!e?.profile) continue;
		const key = profileKey(e.profile);
		if (seen.has(key)) continue;
		seen.add(key);
		cleaned.push({ id: e.id, name: e.name, profile: e.profile });
	}
	profilesState.list = cleaned;
	if (cleaned.length !== list.length) persist();

	lastState.byType = (getSetting('last_by_type') as LastByType | undefined) ?? {};
	lastState.lastType = getSetting('last_type') ?? null;
	lastState.loaded = true;
}

function persist() {
	setSetting('profiles', $state.snapshot(profilesState.list));
}

/** Called on successful connect: adds to history (dedupe by content) and updates per-type last settings */
export function recordUse(profile: Profile) {
	const key = profileKey(profile);
	if (!profilesState.list.some((e) => profileKey(e.profile) === key)) {
		profilesState.list.unshift({
			id: crypto.randomUUID(),
			name: profileTitle(profile),
			profile
		});
	}

	switch (profile.type) {
		case 'serial':
			lastState.byType.serial = profile;
			break;
		case 'ssh':
			lastState.byType.ssh = profile;
			break;
		case 'telnet':
			lastState.byType.telnet = profile;
			break;
		case 'local':
			lastState.byType.local = profile;
			break;
	}
	lastState.lastType = profile.type;

	persist();
	setSetting('last_by_type', $state.snapshot(lastState.byType));
	setSetting('last_type', lastState.lastType);
}

/**
 * Drop a connection from the per-type prefill. Without this, deleting a history
 * entry leaves `last_by_type` holding it and the next launch fills the form with
 * the connection the user just erased — with no way left to reach it.
 */
function forgetLast(profile: Profile) {
	const kind = profile.type;
	const last = lastState.byType[kind];
	if (!last || profileKey(last) !== profileKey(profile)) return;

	const byType = { ...$state.snapshot(lastState.byType) } as LastByType;
	delete byType[kind];
	lastState.byType = byType;
	if (lastState.lastType === kind) lastState.lastType = null;

	setSetting('last_by_type', $state.snapshot(lastState.byType));
	setSetting('last_type', lastState.lastType);
}

export function removeProfile(id: string) {
	const entry = profilesState.list.find((e) => e.id === id);
	profilesState.list = profilesState.list.filter((e) => e.id !== id);
	persist();
	if (entry) forgetLast(entry.profile);
}

export async function renameProfile(id: string, name: string) {
	const entry = profilesState.list.find((e) => e.id === id);
	if (entry && name.trim()) {
		entry.name = name.trim();
		await persist();
	}
}
