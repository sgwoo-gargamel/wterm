import { getSetting, setSetting } from '$lib/persist';

/**
 * Lines sent from any send box — the toolbar's multi-send and every tile share
 * one global history, most recent first.
 */
export const sendHistory = $state<{ list: string[] }>({ list: [] });

const HISTORY_LIMIT = 50;

export function initSendHistory() {
	const saved = getSetting('multi_history');
	if (!Array.isArray(saved)) return;
	sendHistory.list = saved
		.filter((line): line is string => typeof line === 'string' && line.trim().length > 0)
		.slice(0, HISTORY_LIMIT);
}

function persistHistory() {
	setSetting('multi_history', $state.snapshot(sendHistory.list));
}

/** Move a line to the top of the history, dropping the oldest past the limit */
export function recordSend(text: string) {
	if (!text.trim()) return;
	sendHistory.list = [text, ...sendHistory.list.filter((l) => l !== text)].slice(0, HISTORY_LIMIT);
	persistHistory();
}

export function removeHistory(text: string) {
	sendHistory.list = sendHistory.list.filter((l) => l !== text);
	persistHistory();
}

export function clearHistory() {
	sendHistory.list = [];
	persistHistory();
}
