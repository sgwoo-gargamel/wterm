import { listShells, onWslShells, type ShellInfo } from '$lib/ipc';

/**
 * Local shells shared by every connect form. The two sources are kept apart so
 * they cannot race: `builtin` is whatever the command returned, `wsl` is filled
 * in later by the background probe. WSL leads the combined list because the
 * backend puts the default distribution first, and that is the entry a WSL user
 * most likely wants.
 */
const shells = $state<{ builtin: ShellInfo[]; wsl: ShellInfo[] }>({ builtin: [], wsl: [] });

export const shellsState = {
	get list(): ShellInfo[] {
		return [...shells.wsl, ...shells.builtin];
	}
};

let inflight: Promise<void> | null = null;
let listening = false;

/**
 * Populate the shell list. Concurrent callers share one request, and a list
 * already loaded is left alone — several forms opening at once cost one
 * backend call and one WSL probe.
 */
export function loadShells(): Promise<void> {
	if (inflight) return inflight;
	if (shells.builtin.length > 0) return Promise.resolve();
	inflight = (async () => {
		// Awaited before the call: the backend answers a cached probe while the
		// command runs, so a listener registered afterwards would miss it
		if (!listening) {
			listening = true;
			await onWslShells((list) => {
				shells.wsl = list;
			});
		}
		try {
			shells.builtin = await listShells();
		} catch {
			shells.builtin = [];
		}
		inflight = null;
	})();
	return inflight;
}
