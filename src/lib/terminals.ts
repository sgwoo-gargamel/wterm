import { Terminal, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { WebglAddon } from '@xterm/addon-webgl';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { Session } from './stores/sessions.svelte';
import { themeState, type Theme } from './stores/theme.svelte';
import { settingsState } from './stores/settings.svelte';

export const XTERM_THEMES: Record<Theme, ITheme> = {
	dark: {
		background: '#14161c',
		foreground: '#d8dce4',
		cursor: '#6ea3ff',
		selectionBackground: '#33415e'
	},
	light: {
		// Windows 11 light "layer" tone, same as Notepad's editor area
		background: '#f9f9f9',
		foreground: '#1b1b1b',
		cursor: '#2563eb',
		selectionBackground: '#cce0f9',
		// Light-friendly ANSI palette (VS Code Light+ values)
		black: '#000000',
		red: '#cd3131',
		green: '#107c10',
		yellow: '#949800',
		blue: '#0451a5',
		magenta: '#bc05bc',
		cyan: '#0598bc',
		white: '#555555',
		brightBlack: '#666666',
		brightRed: '#cd3131',
		brightGreen: '#14ce14',
		brightYellow: '#b5ba00',
		brightBlue: '#0451a5',
		brightMagenta: '#bc05bc',
		brightCyan: '#0598bc',
		brightWhite: '#a5a5a5'
	}
};

/** Built-in theme for the active mode with the user's custom colors applied */
export function xtermTheme(): ITheme {
	const base = XTERM_THEMES[themeState.theme];
	const { background, foreground, cursor } = themeState.overrides[themeState.theme];
	return {
		...base,
		...(background ? { background } : {}),
		...(foreground ? { foreground } : {}),
		...(cursor ? { cursor } : {})
	};
}

export interface TermBundle {
	term: Terminal;
	fit: FitAddon;
	search: SearchAddon;
}

/**
 * Terminals live outside the component tree, keyed by session id, so layout
 * changes (splitting/adding tiles) can remount views without losing the buffer.
 */
const cache = new Map<string, TermBundle>();

/** Get the cached terminal for a session, creating and wiring it on first use */
export function getTerminal(session: Session): TermBundle {
	const existing = cache.get(session.id);
	if (existing) return existing;

	const term = new Terminal({
		fontFamily: settingsState.font.family,
		fontSize: settingsState.font.size,
		fontWeight: settingsState.font.weight,
		cursorBlink: true,
		scrollback: 10000,
		theme: xtermTheme()
	});
	const fit = new FitAddon();
	const search = new SearchAddon();
	term.loadAddon(fit);
	term.loadAddon(search);

	const encoder = new TextEncoder();
	term.onData((s) => session.write(encoder.encode(s)));

	// PuTTY-style clipboard: selecting text copies it immediately
	term.onSelectionChange(() => {
		const selection = term.getSelection();
		if (selection) void writeText(selection).catch(() => {});
	});

	const bundle: TermBundle = { term, fit, search };
	cache.set(session.id, bundle);
	return bundle;
}

/** Open in the container on first mount; on later mounts move the existing DOM element */
export function attachToContainer(bundle: TermBundle, container: HTMLElement) {
	const { term } = bundle;
	if (term.element) {
		container.appendChild(term.element);
	} else {
		term.open(container);
		try {
			term.loadAddon(new WebglAddon());
		} catch {
			// Fall back to the default renderer where WebGL is unavailable
		}
		// PuTTY-style clipboard: right click pastes (the listener stays with the
		// element across remounts, so wire it only on the initial open).
		// The cast is needed because TS narrowed `element` before open() populated it.
		const el = term.element as HTMLElement | undefined;
		el?.addEventListener('contextmenu', (e: MouseEvent) => {
			e.preventDefault();
			void readText()
				.then((text) => {
					if (text) term.paste(text);
				})
				.catch(() => {});
		});
	}
}

/** Dispose and forget a session's terminal (call when the session is closed for good) */
export function disposeTerminal(sessionId: string) {
	const bundle = cache.get(sessionId);
	if (!bundle) return;
	cache.delete(sessionId);
	try {
		bundle.term.dispose();
	} catch (e) {
		console.error('terminal dispose failed', e);
	}
}
