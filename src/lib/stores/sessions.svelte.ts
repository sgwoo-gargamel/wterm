import { SvelteMap } from 'svelte/reactivity';
import * as ipc from '$lib/ipc';
import type { OutputEvent, Profile } from '$lib/ipc';
import { disposeTerminal } from '$lib/terminals';
import { multiSend } from './multisend.svelte';

export type SessionStatus = 'connected' | 'disconnected';

/** Frontend state for one backend session. Output received before the terminal mounts is buffered. */
export class Session {
	id = '';
	profile: Profile;
	title: string;
	status = $state<SessionStatus>('connected');
	reason = $state('');
	/** Absolute path of the file being written, null when not logging */
	logPath = $state<string | null>(null);
	/** Last known terminal size, for features that need it (e.g. serial stty) */
	cols = 80;
	rows = 24;

	private sink: ((data: Uint8Array) => void) | null = null;
	private buffer: Uint8Array[] = [];

	constructor(profile: Profile) {
		this.profile = profile;
		this.title = ipc.profileTitle(profile);
	}

	handleEvent(ev: OutputEvent) {
		if (ev.type === 'data') {
			const bytes = new Uint8Array(ev.bytes);
			if (this.sink) this.sink(bytes);
			else this.buffer.push(bytes);
		} else if (ev.type === 'disconnected') {
			this.status = 'disconnected';
			this.reason = ev.reason;
		}
	}

	/** Attach the output sink when the terminal view mounts and flush the buffer */
	attach(sink: (data: Uint8Array) => void) {
		this.sink = sink;
		for (const b of this.buffer) sink(b);
		this.buffer = [];
	}

	/**
	 * Detach only if the given sink is still the active one. During layout
	 * changes the new view may attach BEFORE the old view's teardown runs,
	 * and an unconditional detach would silence the fresh attachment.
	 */
	detach(sink: (data: Uint8Array) => void) {
		if (this.sink === sink) this.sink = null;
	}

	write(data: Uint8Array) {
		if (this.status === 'connected' && this.id) void ipc.writeSession(this.id, data);
	}

	resize(cols: number, rows: number) {
		if (this.status === 'connected' && this.id && cols > 0 && rows > 0) {
			this.cols = cols;
			this.rows = rows;
			void ipc.resizeSession(this.id, cols, rows);
		}
	}

	close() {
		if (this.id) void ipc.closeSession(this.id);
	}

	async startLog(path: string, timestamps: boolean, plain: boolean) {
		if (!this.id) return;
		await ipc.startLog(this.id, path, timestamps, plain);
		this.logPath = path;
	}

	stopLog() {
		if (!this.id) return;
		void ipc.stopLog(this.id);
		this.logPath = null;
	}
}

export const sessions = new SvelteMap<string, Session>();

export async function openSession(
	profile: Profile,
	password: string | null,
	cols = 80,
	rows = 24
): Promise<Session> {
	const session = new Session(profile);
	const id = await ipc.openSession(profile, password, cols, rows, (ev) => session.handleEvent(ev));
	session.id = id;
	sessions.set(id, session);
	return session;
}

export function closeSession(id: string) {
	const session = sessions.get(id);
	if (session) {
		session.close();
		sessions.delete(id);
	}
	multiSend.targets.delete(id);
	disposeTerminal(id);
}
