export type Theme = 'dark' | 'light';

/** User overrides for the current theme; unset fields keep the built-in value */
export interface ColorOverrides {
	background?: string;
	foreground?: string;
	cursor?: string;
	accent?: string;
}

export type ColorKey = keyof ColorOverrides;

import { getSetting, setSetting } from '$lib/persist';

export const themeState = $state<{
	theme: Theme;
	overrides: Record<Theme, ColorOverrides>;
}>({ theme: 'dark', overrides: { dark: {}, light: {} } });

/** Overrides that also affect the app chrome (not just the terminal) */
function applyCssOverrides() {
	const { accent, background } = themeState.overrides[themeState.theme];
	const root = document.documentElement;
	root.style.setProperty('--accent', accent ?? '');
	// The terminal fills the pane, so the pane surface follows its background
	root.style.setProperty('--bg-panel', background ?? '');
}

function apply(theme: Theme) {
	document.documentElement.dataset.theme = theme;
	applyCssOverrides();
}

export function initTheme() {
	const saved = getSetting('theme');
	if (saved === 'dark' || saved === 'light') themeState.theme = saved;
	const colors = getSetting('themeColors') as Record<Theme, ColorOverrides> | undefined;
	if (colors) {
		themeState.overrides = { dark: colors.dark ?? {}, light: colors.light ?? {} };
	}
	apply(themeState.theme);
}

export function toggleTheme() {
	themeState.theme = themeState.theme === 'dark' ? 'light' : 'dark';
	setSetting('theme', themeState.theme);
	apply(themeState.theme);
}

function persistColors() {
	setSetting('themeColors', $state.snapshot(themeState.overrides));
	applyCssOverrides();
}

export function setColor(key: ColorKey, value: string) {
	themeState.overrides[themeState.theme][key] = value;
	persistColors();
}

/** Drop custom colors for the current theme */
export function resetColors() {
	themeState.overrides[themeState.theme] = {};
	persistColors();
}
