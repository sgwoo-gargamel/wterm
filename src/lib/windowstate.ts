import { getCurrentWindow } from '@tauri-apps/api/window';
import { getSetting, setSetting } from './persist';

/** Where the window sat last, in physical pixels — Rust puts it back at startup */
export interface WindowState {
	x: number;
	y: number;
	width: number;
	height: number;
	maximized: boolean;
}

function isValid(s: unknown): s is WindowState {
	const w = s as Partial<WindowState> | undefined;
	return (
		!!w &&
		typeof w.x === 'number' &&
		typeof w.y === 'number' &&
		typeof w.width === 'number' &&
		typeof w.height === 'number'
	);
}

/**
 * Remembers the window geometry across restarts. Only the un-maximized geometry
 * is recorded, so un-maximizing after a restart lands back on the real window.
 */
export async function initWindowState() {
	const win = getCurrentWindow();
	const saved = getSetting('window');
	let last: WindowState = isValid(saved)
		? { ...saved, maximized: saved.maximized === true }
		: { x: 0, y: 0, width: 1280, height: 900, maximized: false };

	async function record() {
		try {
			// A minimized window reports a nonsense position (-32000 on Windows)
			if (await win.isMinimized()) return;
			const maximized = await win.isMaximized();
			if (maximized) {
				last = { ...last, maximized: true };
			} else {
				const pos = await win.outerPosition();
				const size = await win.innerSize();
				last = { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized: false };
			}
			setSetting('window', last);
		} catch {
			// A window event racing shutdown is not worth reporting
		}
	}

	let timer: ReturnType<typeof setTimeout> | undefined;
	const schedule = () => {
		clearTimeout(timer);
		timer = setTimeout(() => void record(), 300);
	};

	await win.onMoved(schedule);
	await win.onResized(schedule);
	// Write once so the first run remembers its geometry even if never moved
	await record();
}
