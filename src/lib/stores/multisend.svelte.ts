import { SvelteSet } from 'svelte/reactivity';
import { getSetting, setSetting } from '$lib/persist';
import { sessions } from './sessions.svelte';

/** Session ids that receive input typed in the toolbar's multi-send box */
export const multiSend = $state<{ targets: SvelteSet<string> }>({ targets: new SvelteSet() });

/** Previously sent lines, most recent first */
export const multiHistory = $state<{ list: string[] }>({ list: [] });

const HISTORY_LIMIT = 50;

export function initMultiHistory() {
	const saved = getSetting('multi_history');
	if (!Array.isArray(saved)) return;
	multiHistory.list = saved
		.filter((line): line is string => typeof line === 'string' && line.trim().length > 0)
		.slice(0, HISTORY_LIMIT);
}

function persistHistory() {
	setSetting('multi_history', $state.snapshot(multiHistory.list));
}

/** Move a line to the top of the history, dropping the oldest past the limit */
function recordHistory(text: string) {
	if (!text.trim()) return;
	multiHistory.list = [text, ...multiHistory.list.filter((l) => l !== text)].slice(
		0,
		HISTORY_LIMIT
	);
	persistHistory();
}

export function removeHistory(text: string) {
	multiHistory.list = multiHistory.list.filter((l) => l !== text);
	persistHistory();
}

export function clearHistory() {
	multiHistory.list = [];
	persistHistory();
}

export function toggleTarget(sessionId: string, on: boolean) {
	if (on) multiSend.targets.add(sessionId);
	else multiSend.targets.delete(sessionId);
}

/** Ids of every session that can currently receive input */
function connectedIds(): string[] {
	return [...sessions.values()].filter((s) => s.status === 'connected').map((s) => s.id);
}

export function selectAll() {
	for (const id of connectedIds()) multiSend.targets.add(id);
}

export function clearTargets() {
	multiSend.targets.clear();
}

/** True when every connected session is selected (and there is at least one) */
export function allSelected(): boolean {
	const ids = connectedIds();
	return ids.length > 0 && ids.every((id) => multiSend.targets.has(id));
}

/** Send one line to every selected session; returns how many received it */
export function sendToTargets(text: string): number {
	const encoder = new TextEncoder();
	const data = encoder.encode(`${text}\r`);
	let sent = 0;
	for (const id of multiSend.targets) {
		const session = sessions.get(id);
		if (session && session.status === 'connected') {
			session.write(data);
			sent++;
		}
	}
	// Only lines that actually reached a session are worth keeping
	if (sent > 0) recordHistory(text);
	return sent;
}
