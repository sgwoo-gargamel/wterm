import { listSerialPorts, type PortInfo } from '$lib/ipc';

/** Serial port list shared by the connect form and the recent-connections list */
export const portsState = $state<{ list: PortInfo[]; loaded: boolean }>({
	list: [],
	loaded: false
});

let inflight: Promise<void> | null = null;
let lastRefresh = 0;
const COOLDOWN_MS = 1500;

/**
 * Refresh the port list. Enumeration probe-opens every port to detect busy
 * ones, which is not free — so concurrent calls share one request
 * (single-flight) and non-forced calls within the cooldown are no-ops.
 * Several forms remounting after a tile add then cost one backend call.
 */
export function refreshPorts(force = false): Promise<void> {
	if (inflight) return inflight;
	if (!force && portsState.loaded && Date.now() - lastRefresh < COOLDOWN_MS) {
		return Promise.resolve();
	}
	inflight = (async () => {
		try {
			portsState.list = await listSerialPorts();
		} catch {
			portsState.list = [];
		}
		portsState.loaded = true;
		lastRefresh = Date.now();
		inflight = null;
	})();
	return inflight;
}

/** True when the given port cannot be connected right now (absent or held elsewhere) */
export function portUnavailable(name: string): boolean {
	if (!portsState.loaded) return false;
	const port = portsState.list.find((p) => p.name === name);
	return !port || port.in_use;
}
