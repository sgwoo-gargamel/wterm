<script lang="ts">
	import type { Profile } from '$lib/ipc';
	import { flushSync } from 'svelte';
	import {
		layoutState,
		closePane,
		splitPane,
		swapPanes,
		toggleMaximize,
		countPanes,
		dragState,
		type PaneNode
	} from '$lib/stores/layout.svelte';
	import maximizeIcon from '@fluentui/svg-icons/icons/arrow_maximize_20_regular.svg?raw';
	import minimizeIcon from '@fluentui/svg-icons/icons/arrow_minimize_20_regular.svg?raw';
	import { logBaseName, logTimestamp } from '$lib/ipc';
	import { settingsState } from '$lib/stores/settings.svelte';
	import { multiSend, toggleTarget } from '$lib/stores/multisend.svelte';
	import resizeIcon from '@fluentui/svg-icons/icons/resize_20_regular.svg?raw';
	import saveIcon from '@fluentui/svg-icons/icons/save_20_regular.svg?raw';
	import splitVerticalIcon from '@fluentui/svg-icons/icons/split_vertical_20_regular.svg?raw';
	import splitHorizontalIcon from '@fluentui/svg-icons/icons/split_horizontal_20_regular.svg?raw';
	import dismissIcon from '@fluentui/svg-icons/icons/dismiss_20_regular.svg?raw';
	import { closeSession, openSession, sessions } from '$lib/stores/sessions.svelte';
	import { recordUse, type ProfileEntry } from '$lib/stores/profiles.svelte';
	import { t, translateReason } from '$lib/i18n.svelte';
	import TerminalView from './TerminalView.svelte';
	import SendBox from './SendBox.svelte';
	import ConnectForm from './ConnectForm.svelte';
	import ProfileList from './ProfileList.svelte';
	import HamsterWheel from './HamsterWheel.svelte';

	let { pane }: { pane: PaneNode } = $props();

	const session = $derived(pane.sessionId ? sessions.get(pane.sessionId) : undefined);
	const isActive = $derived(layoutState.activePaneId === pane.id);
	const isMaximized = $derived(layoutState.maximizedPaneId === pane.id);
	// Zooming only means something once the layout has more than one tile
	const canMaximize = $derived(isMaximized || countPanes() > 1);

	let prefill = $state<Profile | null>(null);
	let busy = $state(false);
	let error = $state('');
	/** Non-error status line under the form (cancellation) */
	let notice = $state('');

	// --- Connection progress popup ---
	/** A connection that answers this fast needs no popup; slower ones get the wheel */
	const PROGRESS_DELAY = 400;
	/** Once the popup is up, it stays at least this long so a success right after
	    the delay reads as feedback instead of a flash */
	const PROGRESS_MIN_VISIBLE = 450;
	let progressShownAt = 0;
	let progress = $state<'connecting' | 'failed' | null>(null);
	let progressError = $state('');
	let progressEl = $state<HTMLElement | null>(null);
	/** Bumped on every attempt; a late result whose id no longer matches was cancelled */
	let attempt = 0;

	function activate() {
		layoutState.activePaneId = pane.id;
	}

	async function connect(profile: Profile, password: string | null) {
		const id = ++attempt;
		busy = true;
		error = '';
		notice = '';
		progress = null;
		progressError = '';
		// Backends block until the transport is up (a TCP connect to a dead host can
		// take ~20s), so the popup is the only way out of a stuck attempt
		const timer = setTimeout(() => {
			if (attempt === id) {
				progress = 'connecting';
				progressShownAt = performance.now();
			}
		}, PROGRESS_DELAY);
		try {
			const s = await openSession(profile, password);
			// Hold the popup up to its minimum before revealing the terminal;
			// output arriving meanwhile is buffered by the session
			if (attempt === id && progress === 'connecting') {
				const left = PROGRESS_MIN_VISIBLE - (performance.now() - progressShownAt);
				if (left > 0) await new Promise((resolve) => setTimeout(resolve, left));
			}
			if (attempt !== id) {
				// Cancelled while connecting — the session arrived anyway, drop it
				closeSession(s.id);
				return;
			}
			progress = null;
			pane.sessionId = s.id;
			prefill = null;
			await recordUse(profile);
		} catch (e) {
			if (attempt !== id) return;
			const raw = String(e);
			// Backend sentinel for a missing/invalid local start directory
			const badCwd = raw.startsWith('invalid-cwd:');
			error = badCwd ? `${t('error.invalidCwd')}: ${raw.slice('invalid-cwd:'.length)}` : raw;
			// The popup stays up on failure and waits for a click; a failure that beat
			// the popup is reported inline by the form instead — except a bad start
			// directory, which fails instantly but still deserves the popup
			if (progress === 'connecting' || badCwd) {
				progressError = error === 'password-required' ? t('error.passwordRequired') : error;
				progress = 'failed';
			}
			// SSH: key auth failed — prefill the form so only the password is left to type
			if (profile.type === 'ssh') prefill = { ...profile };
		} finally {
			clearTimeout(timer);
			if (attempt === id) busy = false;
		}
	}

	// A restored workspace hands the pane a connection to make; consume it once.
	// SSH without a stored password lands on the prefilled form, as with history entries.
	$effect(() => {
		const target = pane.pending;
		if (!target) return;
		pane.pending = null;
		void connect(target, null);
	});

	/** Click/Enter/Esc on the popup: cancel while connecting, dismiss once failed */
	function dismissProgress() {
		if (progress === 'connecting') {
			// Abandons the attempt: the pending open is disowned by bumping the id
			attempt++;
			busy = false;
			notice = t('progress.cancelled');
		}
		progress = null;
	}

	// Move focus to the popup so Enter/Esc reach it without a click, and hand focus
	// back to whatever had it (usually the Connect button) once it is dismissed
	$effect(() => {
		if (!progress || !progressEl) return;
		const previous = document.activeElement as HTMLElement | null;
		progressEl.focus();
		if (!previous || previous === progressEl) return;
		return () => {
			if (previous.isConnected) previous.focus();
		};
	});

	function selectHistory(entry: ProfileEntry) {
		// SSH tries key auth first; on failure connect() falls back to the prefilled form
		void connect(entry.profile, null);
	}

	/** Tile send box: one line to this tile only. False keeps it out of the history. */
	function sendLine(text: string): boolean {
		if (!session || session.status !== 'connected') return false;
		session.write(new TextEncoder().encode(`${text}\r`));
		return true;
	}

	/** Serial only: tell a Linux target the terminal size by typing stty at its shell */
	function sendStty() {
		if (!session) return;
		session.write(new TextEncoder().encode(`stty rows ${session.rows} cols ${session.cols}\r`));
	}

	function reconnect() {
		if (!session) return;
		const profile = session.profile;
		closeSession(session.id);
		pane.sessionId = null;
		void connect(profile, null);
	}

	function newConnection() {
		if (session) closeSession(session.id);
		pane.sessionId = null;
		prefill = null;
		error = '';
		notice = '';
	}

	// --- Disconnect overlay: arrow keys move between buttons, Enter activates ---
	let actionsEl = $state<HTMLElement | null>(null);

	function actionButtons(): HTMLElement[] {
		return [...(actionsEl?.querySelectorAll('button') ?? [])] as HTMLElement[];
	}

	// Focus "close tile" (the last action) as soon as the overlay appears
	$effect(() => {
		if (session?.status === 'disconnected' && actionsEl) {
			const buttons = actionButtons();
			buttons[buttons.length - 1]?.focus();
		}
	});

	function onActionsKeydown(e: KeyboardEvent) {
		const buttons = actionButtons();
		if (buttons.length === 0) return;
		const step = ['ArrowRight', 'ArrowDown'].includes(e.key)
			? 1
			: ['ArrowLeft', 'ArrowUp'].includes(e.key)
				? -1
				: 0;
		if (step === 0) return;
		e.preventDefault();
		const current = buttons.indexOf(document.activeElement as HTMLElement);
		const next = (current + step + buttons.length) % buttons.length;
		buttons[next].focus();
	}

	// --- Log saving ---
	let logMenu = $state(false);
	let logMenuEl = $state<HTMLElement | null>(null);
	let logName = $state('');
	let withTimestamp = $state(true);
	let plainText = $state(true);
	let logError = $state('');

	function openLogMenu() {
		if (!session) return;
		if (session.logPath) {
			session.stopLog();
			return;
		}
		logName = logBaseName(session.profile);
		logError = '';
		logMenu = !logMenu;
	}

	async function startLog() {
		if (!session) return;
		if (!settingsState.logDir) {
			logError = t('log.noDir');
			return;
		}
		// The file name always carries the start time so runs never overwrite each other
		const base = logName.trim() || logBaseName(session.profile);
		const file = `${base}_${logTimestamp()}.log`;
		const sep = settingsState.logDir.includes('/') ? '/' : '\\';
		try {
			await session.startLog(`${settingsState.logDir}${sep}${file}`, withTimestamp, plainText);
			logMenu = false;
		} catch (e) {
			logError = String(e);
		}
	}

	// --- Split count menu (2/3/4) ---
	let splitMenu = $state<'row' | 'column' | null>(null);
	let splitMenuEl = $state<HTMLElement | null>(null);

	function pickSplit(count: number) {
		const direction = splitMenu;
		splitMenu = null;
		if (direction) splitPane(pane.id, direction, count);
	}

	// --- Drag & drop tile swap ---
	let dragOver = $state(false);

	function onDragStart(e: DragEvent) {
		dragState.paneId = pane.id;
		e.dataTransfer?.setData('text/plain', pane.id);
		if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
	}

	function onDragEnd() {
		dragState.paneId = null;
		dragOver = false;
	}

	function onDragOver(e: DragEvent) {
		if (!dragState.paneId || dragState.paneId === pane.id) return;
		e.preventDefault();
		if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
		dragOver = true;
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		dragOver = false;
		const from = dragState.paneId;
		dragState.paneId = null;
		if (!from || from === pane.id) return;
		const doSwap = () => swapPanes(from, pane.id);
		// Animate the swap with the View Transitions API when available
		const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
		if (doc.startViewTransition) {
			doc.startViewTransition(() => flushSync(doSwap));
		} else {
			doSwap();
		}
	}
</script>

<svelte:window
	onpointerdown={(e) => {
		if (!splitMenu || !splitMenuEl) return;
		const target = e.target as HTMLElement;
		// Ignore clicks inside the menu and on the split buttons (they toggle themselves)
		if (splitMenuEl.contains(target) || target.closest('.tb')) return;
		splitMenu = null;
	}}
	onpointerdowncapture={(e) => {
		if (!logMenu || !logMenuEl) return;
		const target = e.target as HTMLElement;
		if (logMenuEl.contains(target) || target.closest('.tb')) return;
		logMenu = false;
	}}
/>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="pane"
	class:active={isActive}
	class:dragging={dragState.paneId === pane.id}
	class:drag-over={dragOver}
	style="view-transition-name: {pane.sessionId ? `s-${pane.sessionId}` : `e-${pane.id}`};"
	onpointerdown={activate}
	ondragover={onDragOver}
	ondragleave={() => (dragOver = false)}
	ondrop={onDrop}
>
	<div
		class="title-bar"
		draggable="true"
		ondragstart={onDragStart}
		ondragend={onDragEnd}
	>
		<!-- With no session there is no send box, so the title pushes the buttons right -->
		<span class="title" class:muted={!session} class:solo={!session}>
			{session ? session.title : t('pane.empty')}
		</span>
		{#if session}
			<!-- Send one line to this tile; the history is shared with the toolbar box.
			     draggable=false keeps a click here from starting a tile drag, and the
			     swallowed pointerdown keeps the terminal from grabbing focus back. -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="send-slot" draggable="false" onpointerdown={(e) => e.stopPropagation()}>
				<SendBox
					placeholder={t('send.placeholder')}
					disabled={session.status !== 'connected'}
					compact
					onsend={sendLine}
				/>
			</div>
			{#if session.profile.type === 'serial' && session.status === 'connected'}
				<!-- Serial has no size protocol, so offer a manual stty to the target -->
				<button type="button" class="tb" title={t('pane.sendSize')} onclick={sendStty}>
					{@html resizeIcon}
				</button>
			{/if}
			<!-- Opt this tile into toolbar multi-send -->
			<label class="multi" title={t('multi.target')}>
				<span>{t('multi.label')}</span>
				<input
					type="checkbox"
					checked={multiSend.targets.has(session.id)}
					onchange={(e) => toggleTarget(session.id, e.currentTarget.checked)}
				/>
			</label>
			<button
				type="button"
				class="tb"
				class:logging={!!session.logPath}
				title={session.logPath ? `${t('log.stop')} — ${session.logPath}` : t('log.title')}
				onclick={openLogMenu}
			>
				{@html saveIcon}
			</button>
		{/if}
		{#if logMenu && session}
			<div class="log-menu" bind:this={logMenuEl}>
				<label>
					<span>{t('log.fileName')}</span>
					<input type="text" bind:value={logName} spellcheck="false" />
				</label>
				<label class="check">
					<input type="checkbox" bind:checked={withTimestamp} />
					<span>{t('log.timestamp')}</span>
				</label>
				<label class="check">
					<input type="checkbox" bind:checked={plainText} />
					<span>{t('log.plain')}</span>
				</label>
				<p class="preview">
					{logName || logBaseName(session.profile)}_{logTimestamp()}.log
				</p>
				{#if logError}<p class="log-error">{logError}</p>{/if}
				<button type="button" class="start" onclick={startLog}>{t('log.start')}</button>
			</div>
		{/if}
		<button
			type="button"
			class="tb"
			title={t('pane.splitRight')}
			onclick={() => (splitMenu = splitMenu === 'row' ? null : 'row')}
		>
			{@html splitVerticalIcon}
		</button>
		<button
			type="button"
			class="tb"
			title={t('pane.splitDown')}
			onclick={() => (splitMenu = splitMenu === 'column' ? null : 'column')}
		>
			{@html splitHorizontalIcon}
		</button>
		{#if splitMenu}
			<div class="split-menu" bind:this={splitMenuEl}>
				<span class="split-menu-label">
					{splitMenu === 'row' ? t('pane.splitRight') : t('pane.splitDown')}
				</span>
				{#each [2, 3, 4] as n (n)}
					<button type="button" onclick={() => pickSplit(n)}>{n}</button>
				{/each}
			</div>
		{/if}
		{#if canMaximize}
			<button
				type="button"
				class="tb"
				title={isMaximized ? t('pane.restore') : t('pane.maximize')}
				onclick={() => toggleMaximize(pane.id)}
			>
				{@html isMaximized ? minimizeIcon : maximizeIcon}
			</button>
		{/if}
		<button
			type="button"
			class="tb close"
			title="{t('toolbar.close')} (Ctrl+Shift+W)"
			onclick={() => closePane(pane.id)}
		>
			{@html dismissIcon}
		</button>
	</div>
	{#if session}
		<div class="term-wrap">
			<!-- Keyed so a tile swap remounts the view bound to the right session -->
			{#key session.id}
				<TerminalView {session} active={isActive} onfocused={activate} />
			{/key}
			{#if session.status === 'disconnected'}
				<div class="overlay">
					<p class="reason">{session.reason ? translateReason(session.reason) : t('reason.closed')}</p>
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<div class="actions" role="group" bind:this={actionsEl} onkeydown={onActionsKeydown}>
						<button type="button" onclick={reconnect}>{t('overlay.reconnect')}</button>
						<button type="button" onclick={newConnection}>{t('overlay.newConnection')}</button>
						<button type="button" class="danger" onclick={() => closePane(pane.id)}>
							{t('overlay.closeTile')}
						</button>
					</div>
				</div>
			{/if}
		</div>
	{:else}
		<div class="empty-wrap">
			<div class="empty-pane">
				<div class="columns">
					<ConnectForm {prefill} {busy} onconnect={connect} />
					<ProfileList onselect={selectHistory} />
				</div>
				{#if error}
					<p class="error">
						{error === 'password-required' ? t('error.passwordRequired') : error}
					</p>
				{:else if notice}
					<p class="notice">{notice}</p>
				{/if}
			</div>
			{#if progress}
				<!-- Click anywhere on it: cancels while connecting, dismisses once failed -->
				<div
					class="progress-overlay"
					role="button"
					tabindex="0"
					bind:this={progressEl}
					aria-label={progress === 'failed' ? t('progress.closeHint') : t('progress.cancelHint')}
					onclick={dismissProgress}
					onkeydown={(e) => {
						if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') {
							e.preventDefault();
							dismissProgress();
						}
					}}
				>
					<div class="progress-card">
						<HamsterWheel running={progress === 'connecting'} failed={progress === 'failed'} size="9px" />
						<p class="progress-title" class:failed={progress === 'failed'}>
							{progress === 'failed' ? t('progress.failed') : t('form.connecting')}
						</p>
						{#if progress === 'failed' && progressError}
							<p class="progress-detail">{progressError}</p>
						{/if}
						<p class="progress-hint">
							{progress === 'failed' ? t('progress.closeHint') : t('progress.cancelHint')}
						</p>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.pane {
		display: flex;
		flex-direction: column;
		/* Include the border in 100% size — otherwise the pane overflows its
		   split cell by 2px and the divider paints over the bottom/right border */
		box-sizing: border-box;
		width: 100%;
		height: 100%;
		min-width: 0;
		min-height: 0;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 4px;
		overflow: hidden;
	}
	.pane.active {
		/* Yellow highlight so the active tile stands out at a glance */
		border-color: var(--active);
	}
	.pane.active .title {
		color: var(--active);
	}
	.pane.dragging {
		opacity: 0.45;
	}
	.pane.drag-over {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px var(--accent);
	}
	.title-bar {
		position: relative;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 2px 6px 2px 10px;
		background: var(--bg-elev);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		cursor: grab;
	}
	.tb.logging {
		color: var(--danger);
	}
	.multi {
		display: flex;
		align-items: center;
		gap: 3px;
		font-size: 0.7rem;
		color: var(--fg-icon);
		white-space: nowrap;
		cursor: pointer;
		margin-right: 2px;
	}
	.multi input {
		margin: 0;
		cursor: pointer;
	}
	.log-menu {
		position: absolute;
		top: calc(100% + 2px);
		right: 76px;
		z-index: 40;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		width: 250px;
		padding: 0.6rem;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 6px;
		box-shadow: 0 6px 14px var(--shadow);
		cursor: default;
	}
	.log-menu label {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		font-size: 0.68rem;
		color: var(--fg-muted);
	}
	.log-menu label.check {
		flex-direction: row;
		align-items: center;
		gap: 5px;
	}
	.log-menu input[type='text'] {
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 0.22rem 0.45rem;
		font-size: 0.74rem;
		outline: none;
	}
	.log-menu input[type='text']:focus {
		border-color: var(--border-accent);
	}
	.log-menu .preview {
		margin: 0;
		font-size: 0.66rem;
		color: var(--fg-faint);
		word-break: break-all;
	}
	.log-menu .log-error {
		margin: 0;
		font-size: 0.68rem;
		color: var(--danger);
	}
	.log-menu .start {
		padding: 0.28rem 0;
		background: var(--primary);
		color: var(--primary-fg);
		border: none;
		border-radius: 5px;
		font-size: 0.76rem;
		cursor: pointer;
	}
	.log-menu .start:hover {
		background: var(--primary-hover);
	}
	.split-menu {
		position: absolute;
		top: calc(100% + 2px);
		right: 28px;
		z-index: 40;
		display: flex;
		align-items: center;
		gap: 3px;
		padding: 3px 6px;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 6px;
		box-shadow: 0 6px 14px var(--shadow);
	}
	.split-menu-label {
		font-size: 0.68rem;
		color: var(--fg-muted);
		margin-right: 2px;
		white-space: nowrap;
	}
	.split-menu button {
		width: 24px;
		height: 22px;
		padding: 0;
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 0.76rem;
		cursor: pointer;
	}
	.split-menu button:hover {
		background: var(--accent-soft);
		border-color: var(--border-accent);
	}
	.title-bar:active {
		cursor: grabbing;
	}
	/* Only as wide as its text, and capped so a long one never starves the send box */
	.title {
		flex: 0 1 auto;
		max-width: 40%;
		font-size: 0.85rem;
		/* Same tone as the title-bar icons */
		color: var(--fg-icon);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.title.solo {
		flex: 1 1 auto;
		max-width: none;
	}
	.title.muted {
		color: var(--fg-faint);
	}
	.tb {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 28px;
		padding: 0;
		background: none;
		border: none;
		border-radius: 4px;
		/* Same tone as the toolbar icons */
		color: var(--fg-icon);
		cursor: pointer;
	}
	.tb:hover {
		background: var(--accent-soft);
		color: var(--accent);
	}
	.tb.close:hover {
		background: var(--danger-bg);
		color: var(--danger);
	}
	.tb :global(svg) {
		width: 20px;
		height: 20px;
		fill: currentColor;
		display: block;
	}
	.term-wrap {
		position: relative;
		flex: 1;
		min-height: 0;
	}
	/* Title-bar send box: takes every pixel the buttons and title leave behind */
	.send-slot {
		display: flex;
		flex: 1 1 90px;
		min-width: 0;
		/* Breathing room on both sides, on top of the title bar's own gap */
		margin: 0 6px;
		cursor: default;
	}
	.overlay {
		position: absolute;
		inset: 0;
		/* Must sit above xterm's internal layers (z-index 5-11) so clicks are not intercepted */
		z-index: 20;
		background: var(--overlay);
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.8rem;
	}
	.reason {
		color: var(--fg);
		font-size: 0.9rem;
		margin: 0;
		padding: 0 1rem;
		text-align: center;
	}
	.actions {
		display: flex;
		gap: 8px;
	}
	.actions button {
		padding: 0.4rem 0.9rem;
		background: var(--accent-soft);
		color: var(--fg);
		border: 1px solid var(--border-accent);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.85rem;
	}
	.actions button:hover {
		background: var(--accent-soft-hover);
	}
	.actions button:focus {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
	.actions .danger {
		background: var(--danger-bg);
		border-color: var(--danger-border);
	}
	.actions .danger:hover {
		background: var(--danger-bg-hover);
	}
	/* Positioning context for the connection progress popup */
	.empty-wrap {
		position: relative;
		flex: 1;
		min-height: 0;
		display: flex;
	}
	.empty-pane {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		padding: 1.5rem;
		overflow: auto;
	}
	.progress-overlay {
		position: absolute;
		inset: 0;
		z-index: 25;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--overlay);
		border: none;
		cursor: pointer;
		/* The whole surface is the button; its own focus ring would frame the tile */
		outline: none;
		overflow: hidden;
	}
	.progress-card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.9rem;
		/* Wide enough that the wheel is not cramped, capped so a small tile still fits it */
		min-width: 280px;
		max-width: min(420px, 92%);
		padding: 2rem 2.6rem;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 14px;
		box-shadow: 0 10px 28px var(--shadow);
		pointer-events: none;
	}
	.progress-title {
		margin: 0;
		font-size: 1.05rem;
		color: var(--fg);
	}
	.progress-title.failed {
		color: var(--danger);
	}
	.progress-detail {
		margin: 0;
		max-width: 340px;
		font-size: 0.82rem;
		color: var(--fg-muted);
		text-align: center;
		word-break: break-word;
	}
	.progress-hint {
		margin: 0;
		font-size: 0.8rem;
		color: var(--fg-faint);
	}
	.progress-overlay:focus-visible .progress-card {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px var(--accent);
	}
	.columns {
		display: flex;
		gap: 2.5rem;
		flex-wrap: wrap;
		justify-content: center;
	}
	.error {
		color: var(--danger);
		font-size: 0.85rem;
		max-width: 480px;
		text-align: center;
		margin: 0;
	}
	.notice {
		color: var(--fg-muted);
		font-size: 0.85rem;
		text-align: center;
		margin: 0;
	}
</style>
