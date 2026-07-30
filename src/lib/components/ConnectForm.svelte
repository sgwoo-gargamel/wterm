<script lang="ts">
	import { onMount } from 'svelte';
	import { open } from '@tauri-apps/plugin-dialog';
	import { listShells, type Profile, type ShellInfo } from '$lib/ipc';
	import { lastState, profilesState } from '$lib/stores/profiles.svelte';
	import { portsState, refreshPorts } from '$lib/stores/ports.svelte';
	import { t } from '$lib/i18n.svelte';

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

	// Local shells (WSL distros, PowerShell, cmd) — loaded on first use
	let shells = $state<ShellInfo[]>([]);
	let shellCommand = $state('');
	let shellCwd = $state('');
	const isWsl = $derived(/wsl/i.test(shellCommand));

	/** Switching shells restores the start directory last used with THAT shell */
	function selectShell(command: string) {
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

	async function loadShells() {
		if (shells.length > 0) return;
		try {
			shells = await listShells();
			if (!shellCommand && shells.length > 0) shellCommand = shells[0].command;
		} catch {
			shells = [];
		}
	}

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

	onMount(() => {
		void refreshAndDefault();
		if (kind === 'local') void loadShells();
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

	// Safety net: if the bound port ever ends up empty while devices exist
	// (e.g. a select binding reset when the option list changes), pick the first one
	$effect(() => {
		if (kind === 'serial' && !port && portsState.list.length > 0) {
			port = (portsState.list.find((p) => !p.in_use) ?? portsState.list[0]).name;
		}
	});

	const canSubmit = $derived.by(() => {
		if (busy) return false;
		if (kind === 'local') return shellCommand.trim() !== '';
		if (kind === 'serial') return port.trim() !== '' && !!baudRate && baudRate > 0;
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

<form class="connect-form" onsubmit={submit}>
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
					{#if portsState.list.length === 0 && !port}
						<option value="" disabled>{t('form.noPorts')}</option>
					{/if}
					{#if port && !portsState.list.some((p) => p.name === port)}
						<!-- Saved port not in the current enumeration (unplugged) — shown grayed -->
						<option value={port} disabled>{port}</option>
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
				{#if shells.length === 0}
					<option value="" disabled>{t('form.noDistros')}</option>
				{/if}
				{#each shells as s (s.command)}
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
