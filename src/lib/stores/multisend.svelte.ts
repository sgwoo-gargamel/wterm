import { SvelteSet } from 'svelte/reactivity';
import { sessions } from './sessions.svelte';

/** Session ids that receive input typed in the toolbar's multi-send box */
export const multiSend = $state<{ targets: SvelteSet<string> }>({ targets: new SvelteSet() });

export function toggleTarget(sessionId: string, on: boolean) {
	if (on) multiSend.targets.add(sessionId);
	else multiSend.targets.delete(sessionId);
}

/** Ids of every session that can currently receive input */
function connectedIds(): string[] {
	return [...sessions.values()].filter((s) => s.status === 'connected').map((s) => s.id);
}

/** Sessions that could receive input at all — none means multi-send is pointless */
export function connectedCount(): number {
	return connectedIds().length;
}

/** Selected sessions that are still connected, i.e. what a send would actually reach */
export function targetCount(): number {
	return [...multiSend.targets].filter((id) => sessions.get(id)?.status === 'connected').length;
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
	return sent;
}
