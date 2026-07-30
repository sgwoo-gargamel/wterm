import { getSetting, setSetting } from '$lib/persist';

export interface FontSettings {
	family: string;
	size: number;
	weight: number;
}

export const DEFAULT_FONT: FontSettings = {
	family: '"D2Coding", Consolas, monospace',
	size: 14,
	weight: 400
};

const VALID_WEIGHTS = [300, 400, 500, 700];

export const settingsState = $state<{ font: FontSettings; logDir: string }>({
	font: { ...DEFAULT_FONT },
	logDir: ''
});

export function setLogDir(dir: string) {
	settingsState.logDir = dir;
	setSetting('logDir', dir);
}

export function initSettings() {
	settingsState.logDir = getSetting('logDir') ?? '';
	const saved = getSetting('font') as Partial<FontSettings> | undefined;
	if (!saved) return;
	if (typeof saved.family === 'string' && saved.family.trim()) {
		settingsState.font.family = saved.family;
	}
	if (typeof saved.size === 'number' && saved.size >= 8 && saved.size <= 40) {
		settingsState.font.size = saved.size;
	}
	if (typeof saved.weight === 'number' && VALID_WEIGHTS.includes(saved.weight)) {
		settingsState.font.weight = saved.weight;
	}
}

export function setFont(partial: Partial<FontSettings>) {
	if (partial.family !== undefined && partial.family.trim()) {
		settingsState.font.family = partial.family.trim();
	}
	if (partial.size !== undefined && partial.size >= 8 && partial.size <= 40) {
		settingsState.font.size = partial.size;
	}
	if (partial.weight !== undefined && VALID_WEIGHTS.includes(partial.weight)) {
		settingsState.font.weight = partial.weight;
	}
	setSetting('font', $state.snapshot(settingsState.font));
}

export function resetFont() {
	settingsState.font = { ...DEFAULT_FONT };
	setSetting('font', { ...DEFAULT_FONT });
}
