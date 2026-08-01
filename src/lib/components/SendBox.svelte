<script lang="ts">
	import { t } from '$lib/i18n.svelte';
	import { sendHistory, recordSend, removeHistory, clearHistory } from '$lib/stores/sendhistory.svelte';

	/**
	 * Send box: the visible field is only a trigger. Clicking it opens a panel with
	 * a roomier input and the globally shared history of previously sent lines.
	 * `onsend` reports whether the line was actually delivered — only those are kept.
	 */
	let {
		placeholder,
		disabled = false,
		sendDisabled = undefined,
		compact = false,
		onsend
	}: {
		placeholder: string;
		/** Turns the whole box off: the trigger stops opening the panel */
		disabled?: boolean;
		/** Greys out just the Send button, e.g. when nothing is selected to receive */
		sendDisabled?: boolean;
		/**
		 * Title-bar sizing: the trigger fills the space it is given and the panel
		 * spans the nearest positioned ancestor (the tile title bar) instead of the box
		 */
		compact?: boolean;
		onsend: (text: string) => boolean;
	} = $props();

	let text = $state('');
	let trigger = $state<HTMLInputElement | null>(null);
	let panelInput = $state<HTMLInputElement | null>(null);
	let panel = $state<HTMLElement | null>(null);
	let open = $state(false);
	// Position while walking the history with the arrow keys; -1 means "editing a new line"
	let historyIndex = $state(-1);
	const noSend = $derived(sendDisabled ?? disabled);

	// A box that goes dead while its panel is open takes the panel with it
	$effect(() => {
		if (disabled) open = false;
	});

	function openPanel() {
		if (disabled) return;
		open = true;
		// The panel input only exists once the panel has rendered
		void Promise.resolve().then(() => panelInput?.focus());
	}

	function submit() {
		if (noSend) return;
		if (onsend(text)) recordSend(text);
		text = '';
		historyIndex = -1;
	}

	function onPanelKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			open = false;
			return;
		}
		if (e.key === 'Enter') {
			e.preventDefault();
			submit();
			return;
		}
		// Up/Down walk previously sent lines, like a shell prompt
		if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
		const list = sendHistory.list;
		if (list.length === 0) return;
		e.preventDefault();
		const next = e.key === 'ArrowUp' ? historyIndex + 1 : historyIndex - 1;
		historyIndex = Math.min(Math.max(next, -1), list.length - 1);
		text = historyIndex === -1 ? '' : list[historyIndex];
	}

	/** Clicking a history line loads it for editing; sending stays an explicit step */
	function useHistory(line: string) {
		text = line;
		historyIndex = -1;
		panelInput?.focus();
	}
</script>

<svelte:window
	onpointerdowncapture={(e) => {
		// Capture phase: a tile swallows pointerdown over its own send box, which would
		// otherwise leave another tile's panel open
		if (!open) return;
		const target = e.target as Node;
		if (panel?.contains(target) || target === trigger) return;
		open = false;
	}}
/>

<div class="send-box" class:compact>
	<input
		type="text"
		class="trigger"
		{placeholder}
		spellcheck="false"
		readonly
		{disabled}
		bind:this={trigger}
		value={text}
		onfocus={openPanel}
		onclick={openPanel}
	/>
	{#if !compact}
		<button type="button" class="send" title={t('multi.send')} disabled={noSend} onclick={submit}>
			{t('multi.send')}
		</button>
	{/if}
	{#if open}
		<div class="panel" bind:this={panel}>
			<div class="panel-row">
				<input
					type="text"
					class="panel-input"
					{placeholder}
					spellcheck="false"
					bind:this={panelInput}
					bind:value={text}
					onkeydown={onPanelKeydown}
				/>
				<button
					type="button"
					class="send"
					title={t('multi.send')}
					disabled={noSend}
					onclick={submit}
				>
					{t('multi.send')}
				</button>
			</div>
			<span class="history-title">{t('multi.history')}</span>
			{#if sendHistory.list.length === 0}
				<p class="history-empty">{t('multi.historyEmpty')}</p>
			{:else}
				<ul>
					{#each sendHistory.list as line (line)}
						<li>
							<button type="button" class="entry" onclick={() => useHistory(line)}>
								{line}
							</button>
							<button
								type="button"
								class="del"
								title={t('multi.historyRemove')}
								onclick={() => removeHistory(line)}
							>
								✕
							</button>
						</li>
					{/each}
				</ul>
				<div class="foot">
					<button type="button" class="clear" onclick={clearHistory}>
						{t('multi.historyClear')}
					</button>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.send-box {
		position: relative;
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.trigger {
		width: 250px;
		height: 28px;
		box-sizing: border-box;
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 0 8px;
		font-family: inherit;
		font-size: 0.85rem;
		outline: none;
		cursor: pointer;
	}
	.trigger:focus {
		border-color: var(--border-accent);
	}
	.trigger:disabled {
		opacity: 0.45;
		cursor: default;
	}
	/* In a title bar the trigger takes the width it is given */
	.compact {
		/* Not a positioning context, so the panel spans the whole title bar below */
		position: static;
		width: 100%;
	}
	.compact .trigger {
		width: 100%;
		height: 22px;
		font-size: 0.78rem;
		padding: 0 6px;
	}
	/* The tile clips overflow, so the panel can only be as wide as the tile */
	.compact .panel {
		top: calc(100% + 2px);
		left: 5px;
		right: 5px;
		width: auto;
	}
	.compact ul {
		max-height: 180px;
	}
	.send {
		background: var(--primary);
		color: var(--primary-fg);
		border: 1px solid var(--primary);
		border-radius: 5px;
		padding: 3px 12px;
		font-size: 0.8rem;
		cursor: pointer;
	}
	.send:hover:not(:disabled) {
		background: var(--primary-hover);
		border-color: var(--primary-hover);
	}
	.send:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.panel {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		z-index: 50;
		width: 460px;
		max-width: 100vw;
		padding: 0.5rem;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 8px;
		box-shadow: 0 8px 20px var(--shadow);
	}
	.panel-row {
		display: flex;
		align-items: center;
		gap: 5px;
	}
	/* The real input: roomier than the trigger it replaces */
	.panel-input {
		flex: 1;
		min-width: 0;
		height: 40px;
		box-sizing: border-box;
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border-accent);
		border-radius: 6px;
		padding: 0 10px;
		font-family: inherit;
		font-size: 1rem;
		outline: none;
	}
	.history-title {
		display: block;
		margin: 0.55rem 0 0.3rem;
		padding-top: 0.45rem;
		border-top: 1px solid var(--border);
		font-size: 0.68rem;
		font-weight: 600;
		color: var(--fg-muted);
	}
	.history-empty {
		margin: 0;
		padding: 0.25rem;
		font-size: 0.74rem;
		color: var(--fg-faint);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
		max-height: 300px;
		overflow-y: auto;
	}
	li {
		display: flex;
		gap: 3px;
		align-items: stretch;
	}
	.entry {
		flex: 1;
		min-width: 0;
		padding: 0.28rem 0.5rem;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 5px;
		color: var(--fg);
		font-family: inherit;
		font-size: 0.78rem;
		text-align: left;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		cursor: pointer;
	}
	.entry:hover {
		border-color: var(--border-accent);
	}
	.del {
		background: none;
		border: none;
		color: var(--fg-faint);
		padding: 0 5px;
		font-size: 0.78rem;
		border-radius: 5px;
		cursor: pointer;
	}
	.del:hover {
		color: var(--danger);
		background: var(--bg-input);
	}
	.foot {
		display: flex;
		justify-content: flex-end;
		margin-top: 0.35rem;
		padding-top: 0.35rem;
		border-top: 1px solid var(--border);
	}
	.clear {
		background: none;
		border: none;
		font-size: 0.72rem;
		color: var(--fg-muted);
		cursor: pointer;
	}
	.clear:hover {
		color: var(--danger);
	}
</style>
