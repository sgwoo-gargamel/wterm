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
		// Kept at the app's panel tone rather than VS Code's #191A1B so the
		// terminal and the pane chrome stay on one surface colour
		background: '#14161c',
		// Deliberately below pure white: near-white on near-black halates and
		// reads as blurry. One step below VS Code's #CCCCCC (~11:1 here) —
		// ~10:1 against our background trades a little contrast for visibly
		// less glow around thin strokes on dark.
		foreground: '#c0c0c0',
		cursor: '#6ea3ff',
		selectionBackground: '#3994bc33',
		// VS Code dark ANSI palette (terminal.ansi* defaults)
		black: '#000000',
		red: '#cd3131',
		green: '#0dbc79',
		yellow: '#e5e510',
		blue: '#2472c8',
		magenta: '#bc3fbc',
		cyan: '#11a8cd',
		white: '#e5e5e5',
		brightBlack: '#666666',
		brightRed: '#f14c4c',
		brightGreen: '#23d18b',
		brightYellow: '#f5f543',
		brightBlue: '#3b8eea',
		brightMagenta: '#d670d6',
		brightCyan: '#29b8db',
		brightWhite: '#e5e5e5'
	},
	light: {
		// Neutral gray instead of Windows' near-white layer tone (#f9f9f9):
		// full-brightness white at terminal size is glaring; this keeps ~13:1
		// against the foreground so sharpness doesn't suffer. Must stay in sync
		// with --bg-panel (light) — the container padding shows that colour.
		background: '#f0efed',
		foreground: '#1b1b1b',
		cursor: '#2563eb',
		selectionBackground: '#cce0f9',
		// VS Code light ANSI palette (terminal.ansi* defaults)
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
	/** Present while the WebGL renderer is active; absent on the DOM renderer */
	webgl?: WebglAddon;
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
		// PuTTY-style bold: SGR 1 shows as a bright colour, not a heavier face.
		// Synthesised bold smears badly in D2Coding at typical terminal sizes.
		fontWeightBold: settingsState.font.weight,
		// Matches VS Code's terminal.integrated.minimumContrastRatio default:
		// nudge ANSI colours that land too close to the background so they stay
		// legible. xterm's own default of 1 leaves them as sent.
		minimumContrastRatio: 4.5,
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

/**
 * Renderer trade-off: WebGL is fast under heavy output but canvas glyphs only
 * get grayscale AA; the DOM renderer gets ClearType subpixel AA (sharp next to
 * native Windows text) at the cost of throughput. User-selectable in settings.
 */
function applyRenderer(bundle: TermBundle) {
	const wantWebgl = settingsState.renderer === 'webgl';
	if (wantWebgl && !bundle.webgl && bundle.term.element) {
		try {
			bundle.webgl = new WebglAddon();
			bundle.term.loadAddon(bundle.webgl);
		} catch {
			// Fall back to the DOM renderer where WebGL is unavailable
			bundle.webgl = undefined;
		}
	} else if (!wantWebgl && bundle.webgl) {
		// Disposing the addon drops the terminal back to the DOM renderer
		bundle.webgl.dispose();
		bundle.webgl = undefined;
	}
}

/** Switch every live terminal to the renderer currently selected in settings */
export function applyRendererToAll() {
	for (const bundle of cache.values()) applyRenderer(bundle);
}

/** Open in the container on first mount; on later mounts move the existing DOM element */
export function attachToContainer(bundle: TermBundle, container: HTMLElement) {
	const { term } = bundle;
	if (term.element) {
		container.appendChild(term.element);
	} else {
		term.open(container);
		applyRenderer(bundle);
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

/** Clear a session's screen and scrollback, keeping only the current bottom line */
export function clearTerminal(sessionId: string) {
	const term = cache.get(sessionId)?.term;
	if (!term) return;
	term.clear();
	// Invoked from a title-bar button — hand focus back to the terminal so
	// typing resumes immediately instead of re-triggering the button on Enter
	term.focus();
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
