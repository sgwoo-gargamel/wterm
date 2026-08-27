<script lang="ts">
	import { onMount } from 'svelte';
	import { open } from '@tauri-apps/plugin-dialog';
	import HamsterWheel from './HamsterWheel.svelte';
	import { type Profile } from '$lib/ipc';
	import { lastState, profilesState } from '$lib/stores/profiles.svelte';
	import { portsState, refreshPorts } from '$lib/stores/ports.svelte';
	import { shellsState, loadShells } from '$lib/stores/shells.svelte';
	import { t, type MessageKey } from '$lib/i18n.svelte';

	let {
		prefill = null,
		busy = false,
		onconnect
	}: {
		prefill?: Profile | null;
		busy?: boolean;
		onconnect: (profile: Profile, password: string | null) => void;
	} = $props();

	type Kind = 'serial' | 'ssh' | 'telnet' | 'local';
	let kind = $state<Kind>('serial');

	// The warm-up holds for a beat even when the lists come back at once —
	// a spinner that vanishes on sight reads as a glitch rather than progress.
	const WARMUP_MIN_MS = 700;
	/** Ceiling on the warm-up, so a stalled probe still gives up the form */
	const WARMUP_CAP_MS = 1500;
	/** How long the finished card stays up before handing over to the form */
	const WARMUP_DONE_MS = 500;

	let shellCommand = $state('');
	let shellCwd = $state('');
	const isWsl = $derived(/wsl/i.test(shellCommand));
	// The selection is made once, from whichever shells arrived first, and then
	// left alone. Following the list instead would re-select when the WSL entries
	// land a moment later, and the visible jump reads as a flicker.
	let shellPicked = $state(false);

	/** Switching shells restores the start directory last used with THAT shell */
	function selectShell(command: string) {
		shellPicked = true;
		shellCommand = command;
		const previous = profilesState.list.find(
			(e) => e.profile.type === 'local' && e.profile.command === command
		);
		shellCwd =
			previous?.profile.type === 'local' ? (previous.profile.cwd ?? '') : '';
	}

	async function pickCwd() {
		const dir = await open({
			directory: true,
			multiple: false,
			defaultPath: shellCwd || undefined
		});
		if (typeof dir === 'string') shellCwd = dir;
	}

	// Plain flag, not state: writing state this effect reads would re-run it
	let shellDefaulted = false;
	$effect(() => {
		// Waiting for the probe keeps the WSL distribution as the default —
		// picking from a half-filled list would settle on PowerShell instead
		if (shellDefaulted || shellPicked || !shellsState.settled) return;
		const first = shellsState.list[0];
		if (!first) return;
		shellDefaulted = true;
		shellCommand = first.command;
	});

	// serial
	let port = $state('');
	const BAUD_RATES = [921600, 115200, 57600, 38400, 19200, 9600];
	let baudRate = $state<number | null>(115200);
	// Editable combobox: the input takes typed values, the arrow opens the preset list
	let baudListOpen = $state(false);
	let baudCombo = $state<HTMLDivElement | null>(null);

	// ssh / telnet
	let host = $state('');
	let netPort = $state(22);
	let username = $state('');
	let password = $state('');
	let passwordInput = $state<HTMLInputElement | null>(null);

	function selectKind(k: Kind) {
		if (kind === k) return;
		// Prefill the tab with the last-connected settings for that type, if any
		const last = lastState.byType[k];
		if (last) {
			applyPrefill(last);
		} else {
			kind = k;
			if (k === 'ssh' && (netPort === 23 || !netPort)) netPort = 22;
			if (k === 'telnet' && (netPort === 22 || !netPort)) netPort = 23;
		}
		// The device list may have changed while another tab was showing
		if (k === 'serial') void refreshAndDefault();
		if (k === 'local') void loadShells();
	}

	async function refreshAndDefault(force = false) {
		await refreshPorts(force);
		// Default only when nothing is selected — never overwrite a saved port.
		// Prefer the first port that is actually connectable.
		const list = portsState.list;
		if (!port && list.length > 0) {
			port = (list.find((p) => !p.in_use) ?? list[0]).name;
		}
	}

	function applyPrefill(p: Profile) {
		kind = p.type;
		if (p.type === 'local') {
			// A saved profile names its shell, so the list must not override it
			shellPicked = true;
			shellCommand = p.command;
			shellCwd = p.cwd ?? '';
			void loadShells();
		} else if (p.type === 'serial') {
			port = p.port;
			baudRate = p.baud_rate;
		} else {
			host = p.host;
			netPort = p.port;
			username = p.username ?? '';
		}
	}

	/**
	 * Devices come and go between runs, so every launch scans afresh and the
	 * warm-up always covers it. A form opened later in the session has nothing
	 * to wait for — the lists were gathered at startup — and skips it outright.
	 */
	const listsPending = !(portsState.loaded && shellsState.settled);
	let warmedUp = $state(!listsPending);
	let minShown = $state(false);
	let scanDone = $state(false);
	$effect(() => {
		if (listsPending && minShown && portsState.loaded && shellsState.settled) scanDone = true;
	});

	// The wheel stops and the card says so before it goes, so the overlay is not
	// yanked away mid-spin
	$effect(() => {
		if (!scanDone) return;
		const done = setTimeout(() => (warmedUp = true), WARMUP_DONE_MS);
		return () => clearTimeout(done);
	});

	onMount(() => {
		void refreshAndDefault();
		// Both lists are needed up front: the warm-up waits on either one
		void loadShells();
		const min = setTimeout(() => (minShown = true), WARMUP_MIN_MS);
		// Routed through the same completion beat, so even a timed-out scan ends
		// the same way instead of blinking out
		const cap = setTimeout(() => (scanDone = true), WARMUP_CAP_MS);
		return () => {
			clearTimeout(min);
			clearTimeout(cap);
		};
	});

	$effect(() => {
		if (prefill) {
			applyPrefill(prefill);
			// When an SSH history entry is picked, focus the password so it is all that is left to type
			if (prefill.type === 'ssh') setTimeout(() => passwordInput?.focus(), 0);
		}
	});

	// Once per form: open on the last-used transport type with its last settings prefilled.
	// An explicit prefill (history selection) takes priority.
	let appliedInitial = false;
	$effect(() => {
		if (appliedInitial || prefill || !lastState.loaded) return;
		appliedInitial = true;
		const initialKind = lastState.lastType ?? kind;
		const last = lastState.byType[initialKind];
		if (last) {
			applyPrefill(last);
			void refreshAndDefault();
		}
	});

	// Deleting the history entry that put an absent port here should clear the
	// field now, not only on the next launch. Narrow on purpose: the port must be
	// unusable AND no longer the stored prefill, which only a deletion produces.
	$effect(() => {
		if (kind !== 'serial' || !port || !portIssue) return;
		if (lastState.byType.serial?.port === port) return;
		port = '';
	});

	// Safety net: if the bound port ever ends up empty while devices exist
	// (e.g. a select binding reset when the option list changes), pick the first one
	$effect(() => {
		if (kind === 'serial' && !port && portsState.list.length > 0) {
			port = (portsState.list.find((p) => !p.in_use) ?? portsState.list[0]).name;
		}
	});

	/**
	 * Why the selected port cannot be opened, or null when it can. Silent until
	 * enumeration has run — before that an absent port only means "not yet known".
	 */
	const portIssue = $derived.by((): MessageKey | null => {
		if (kind !== 'serial' || !port || !portsState.loaded) return null;
		const found = portsState.list.find((p) => p.name === port);
		if (!found) return 'form.portMissing';
		return found.in_use ? 'form.portBusy' : null;
	});

	const canSubmit = $derived.by(() => {
		if (busy) return false;
		if (kind === 'local') return shellCommand.trim() !== '';
		// Connecting to an unplugged or busy port only fails later, with a popup
		if (kind === 'serial') return port.trim() !== '' && !!baudRate && baudRate > 0 && !portIssue;
		if (kind === 'ssh') return host.trim() !== '' && netPort > 0 && username.trim() !== '';
		return host.trim() !== '' && netPort > 0;
	});

	function submit(e: Event) {
		e.preventDefault();
		if (!canSubmit) return;
		if (kind === 'local') {
			// WSL keeps its own home directory, so no cwd is sent for it
			onconnect(
				{
					type: 'local',
					command: shellCommand.trim(),
					cwd: isWsl ? null : shellCwd.trim() || null
				},
				null
			);
		} else if (kind === 'serial') {
			onconnect({ type: 'serial', port: port.trim(), baud_rate: Number(baudRate) }, null);
		} else if (kind === 'ssh') {
			onconnect(
				{ type: 'ssh', host: host.trim(), port: Number(netPort), username: username.trim() },
				password
			);
		} else {
			onconnect(
				{
					type: 'telnet',
					host: host.trim(),
					port: Number(netPort),
					username: username.trim() || null
				},
				null
			);
		}
	}
</script>

<svelte:window
	onpointerdown={(e) => {
		if (baudListOpen && baudCombo && !baudCombo.contains(e.target as Node)) baudListOpen = false;
	}}
/>

{#if !warmedUp}
	<!-- Absolute against Pane's .empty-wrap, so it centres over the whole tile
	     exactly like the connecting card rather than sitting in the form column -->
	<div class="warmup-overlay">
		<div class="warmup-card">
			<HamsterWheel running={!scanDone} size="9px" />
			<p>{scanDone ? t('form.scanDone') : t('form.scanning')}</p>
		</div>
	</div>
{/if}
	<!-- Kept in the layout while hidden, so nothing shifts when it takes over -->
	<form class="connect-form" class:warming={!warmedUp} onsubmit={submit}>
	<div class="tabs" role="tablist">
		{#each [['serial', t('tab.serial')], ['ssh', 'SSH'], ['telnet', 'Telnet'], ['local', t('tab.local')]] as [k, label] (k)}
			<button
				type="button"
				role="tab"
				class:selected={kind === k}
				aria-selected={kind === k}
				onclick={() => selectKind(k as Kind)}
			>
				{label}
			</button>
		{/each}
	</div>

	{#if kind === 'serial'}
		<label>
			<span>{t('form.port')}</span>
			<span class="row">
				<select bind:value={port}>
					<!-- Enumeration probe-opens every port, so the list lands late. Until
					     it does, say so rather than claiming there are none. -->
					{#if !port && !portsState.loaded}
						<option value="" disabled>{t('form.loadingPorts')}</option>
					{:else if !port && portsState.list.length === 0}
						<option value="" disabled>{t('form.noPorts')}</option>
					{/if}
					{#if port && !portsState.list.some((p) => p.name === port)}
						<!-- A saved port keeps its own entry so the binding never loses its
						     match and resets. Grayed as unplugged only once enumeration has
						     actually run and come back without it. -->
						<option value={port} disabled={portsState.loaded}>{port}</option>
					{/if}
					{#each portsState.list as p (p.name)}
						<option value={p.name} disabled={p.in_use}>
							{p.name}{p.kind ? ` — ${p.kind}` : ''}{p.in_use ? ` (${t('form.portInUse')})` : ''}
						</option>
					{/each}
				</select>
				<button
					type="button"
					class="icon"
					title={t('form.refreshPorts')}
					onclick={() => void refreshAndDefault(true)}
				>
					↻
				</button>
			</span>
		</label>
		<!-- Says why the connect button is off; ↻ is right there to retry after replugging -->
		{#if portIssue}
			<p class="port-issue">{t(portIssue)}</p>
		{/if}
		<label>
			<span>{t('form.baudRate')}</span>
			<div class="baud-combo" bind:this={baudCombo}>
				<input
					type="number"
					bind:value={baudRate}
					min="1"
					onfocus={() => (baudListOpen = false)}
				/>
				<button
					type="button"
					class="arrow"
					aria-label={t('form.baudRate')}
					onclick={() => (baudListOpen = !baudListOpen)}
				>
					▾
				</button>
				{#if baudListOpen}
					<ul class="baud-list" role="listbox">
						{#each BAUD_RATES as b (b)}
							<li>
								<button
									type="button"
									role="option"
									aria-selected={baudRate === b}
									class:selected={baudRate === b}
									onclick={() => {
										baudRate = b;
										baudListOpen = false;
									}}
								>
									{b}
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		</label>
		<!-- Ghost row keeps the form height equal to the SSH tab (password line) -->
		<label class="ghost" aria-hidden="true">
			<span>&nbsp;</span>
			<input tabindex="-1" disabled />
		</label>
	{:else if kind === 'local'}
		<label>
			<span>{t('form.shell')}</span>
			<select
				value={shellCommand}
				onchange={(e) => selectShell(e.currentTarget.value)}
			>
				{#if shellsState.list.length === 0}
					<option value="" disabled>{t('form.noDistros')}</option>
				{/if}
				{#each shellsState.list as s (s.command)}
					<option value={s.command}>{s.label}</option>
				{/each}
			</select>
		</label>
		<label class:disabled={isWsl}>
			<span>{t('form.startDir')}</span>
			<span class="row">
				<input
					type="text"
					bind:value={shellCwd}
					spellcheck="false"
					disabled={isWsl}
					placeholder={isWsl ? t('form.startDirWsl') : t('form.startDirHome')}
				/>
				<button
					type="button"
					class="icon"
					title={t('form.browse')}
					disabled={isWsl}
					onclick={pickCwd}
				>
					…
				</button>
			</span>
		</label>
		<!-- Ghost row keeps the form height equal to the other tabs -->
		<label class="ghost" aria-hidden="true">
			<span>&nbsp;</span>
			<input tabindex="-1" disabled />
		</label>
	{:else}
		<div class="host-row">
			<label class="host-col">
				<span>{t('form.host')}</span>
				<input type="text" bind:value={host} placeholder="192.168.0.10" spellcheck="false" />
			</label>
			<label class="port-col">
				<span>{t('form.port')}</span>
				<input type="number" bind:value={netPort} min="1" max="65535" />
			</label>
		</div>
		<label>
			<span>{t('form.username')}{kind === 'telnet' ? t('form.usernameOptional') : ''}</span>
			<input type="text" bind:value={username} spellcheck="false" autocapitalize="off" />
		</label>
		{#if kind === 'ssh'}
			<label>
				<span>{t('form.password')}</span>
				<input type="password" bind:value={password} bind:this={passwordInput} />
			</label>
		{:else}
			<!-- Ghost row keeps the form height equal to the SSH tab (password line) -->
			<label class="ghost" aria-hidden="true">
				<span>&nbsp;</span>
				<input tabindex="-1" disabled />
			</label>
		{/if}
	{/if}

	<button type="submit" class="primary" disabled={!canSubmit}>
		{busy ? t('form.connecting') : t('form.connect')}
	</button>
	</form>

<style>
	.connect-form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		/* Fixed width so switching tabs never shifts the layout */
		width: 270px;
	}
	.connect-form.warming {
		visibility: hidden;
	}
	.port-issue {
		margin: -0.2rem 0 0;
		color: var(--danger);
		font-size: 0.78rem;
	}
	/* Mirrors .progress-overlay / .progress-card in Pane so both indicators read
	   as the same thing; not interactive, since there is nothing to cancel */
	.warmup-overlay {
		position: absolute;
		inset: 0;
		z-index: 25;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--overlay);
		overflow: hidden;
	}
	.warmup-card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.9rem;
		min-width: 280px;
		max-width: min(420px, 92%);
		padding: 2rem 2.6rem;
		background: var(--popup-bg);
		border: 1px solid var(--popup-border);
		border-radius: 14px;
		box-shadow: 0 10px 28px var(--shadow);
	}
	.warmup-card p {
		margin: 0;
		font-size: 1.05rem;
		color: var(--popup-fg);
	}
	.ghost {
		visibility: hidden;
	}
	label.disabled {
		opacity: 0.45;
	}
	input:disabled,
	.icon:disabled {
		cursor: default;
	}
	label > span {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.tabs {
		display: flex;
		gap: 4px;
		margin-bottom: 0.3rem;
	}
	.tabs button {
		flex: 1;
		padding: 0.3rem 0;
		font-size: 0.78rem;
		background: var(--bg-input);
		color: var(--fg-muted);
		border: 1px solid var(--border);
		border-radius: 6px;
		cursor: pointer;
	}
	.tabs button.selected {
		background: var(--accent-soft);
		color: var(--fg);
		border-color: var(--border-accent);
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		font-size: 0.72rem;
		color: var(--fg-muted);
	}
	/* Every tab shows three rows; fixing their height keeps the vertically
	   centred form from shifting when switching tabs. Tall enough that the
	   28px controls (and their focus rings) are never clipped. */
	.connect-form > label,
	.connect-form > .host-row {
		height: 50px;
		box-sizing: border-box;
		overflow: visible;
	}
	.row {
		display: flex;
		gap: 4px;
	}
	.host-row {
		display: flex;
		gap: 8px;
	}
	.host-col {
		flex: 1;
		min-width: 0;
	}
	.port-col {
		width: 72px;
		flex-shrink: 0;
	}
	.host-row input {
		width: 100%;
		min-width: 0;
		box-sizing: border-box;
	}
	.row select,
	.row input {
		flex: 1;
		min-width: 0;
	}
	input,
	select {
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 0.3rem 0.5rem;
		font-size: 0.8rem;
		outline: none;
		/* Explicit height so <select> and <input> rows match exactly across tabs */
		height: 28px;
		box-sizing: border-box;
	}
	input:focus,
	select:focus {
		border-color: var(--border-accent);
	}
	.icon {
		background: var(--bg-input);
		color: var(--fg-muted);
		border: 1px solid var(--border);
		border-radius: 5px;
		width: 28px;
		height: 28px;
		box-sizing: border-box;
		cursor: pointer;
	}
	.baud-combo {
		position: relative;
		display: flex;
	}
	.baud-combo input {
		flex: 1;
		min-width: 0;
		padding-right: 28px;
		/* Hide the number spinner so the combo arrow is the only control on the right */
		appearance: textfield;
	}
	.baud-combo input::-webkit-outer-spin-button,
	.baud-combo input::-webkit-inner-spin-button {
		appearance: none;
		margin: 0;
	}
	.arrow {
		position: absolute;
		top: 1px;
		right: 1px;
		bottom: 1px;
		width: 24px;
		background: var(--bg-input);
		color: var(--fg-muted);
		border: none;
		border-left: 1px solid var(--border);
		border-radius: 0 6px 6px 0;
		cursor: pointer;
	}
	.arrow:hover {
		color: var(--fg);
	}
	.baud-list {
		position: absolute;
		top: calc(100% + 2px);
		left: 0;
		right: 0;
		margin: 0;
		padding: 4px;
		list-style: none;
		background: var(--bg-input);
		border: 1px solid var(--border-accent);
		border-radius: 6px;
		z-index: 30;
		max-height: 220px;
		overflow-y: auto;
		box-shadow: 0 6px 16px var(--shadow);
	}
	.baud-list button {
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		color: var(--fg);
		padding: 0.28rem 0.5rem;
		border-radius: 4px;
		cursor: pointer;
		font-size: 0.8rem;
	}
	.baud-list button:hover {
		background: var(--accent-soft);
	}
	.baud-list button.selected {
		color: var(--accent);
		font-weight: 600;
	}
	.icon:hover {
		color: var(--fg);
	}
	.primary {
		margin-top: 0.3rem;
		padding: 0.42rem 0;
		background: var(--primary);
		color: var(--primary-fg);
		border: none;
		border-radius: 5px;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.primary:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.primary:not(:disabled):hover {
		background: var(--primary-hover);
	}
</style>
