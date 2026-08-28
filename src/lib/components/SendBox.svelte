<script lang="ts">
	import { t } from '$lib/i18n.svelte';
	import { sendHistory, recordSend, removeHistory, clearHistory } from '$lib/stores/sendhistory.svelte';

	/**
	 * Send box: the visible field is where the line is typed. Focusing it drops down
	 * the globally shared history of previously sent lines, nothing else.
	 * `onsend` reports whether the line was actually delivered — only those are kept.
	 */
	let {
		placeholder,
		disabled = false,
		sendDisabled = undefined,
		compact = false,
		fill = false,
		onsend
	}: {
		placeholder: string;
		/** Turns the whole box off: the trigger stops opening the panel */
		disabled?: boolean;
		/** Greys out just the Send button, e.g. when nothing is selected to receive */
		sendDisabled?: boolean;
		/**
		 * Title-bar sizing: the input fills the space it is given and the panel
		 * spans the nearest positioned ancestor (the tile title bar) instead of the box
		 */
		compact?: boolean;
		/** Toolbar sizing: the box stretches over whatever width its row can spare */
		fill?: boolean;
		onsend: (text: string) => boolean;
	} = $props();

	let text = $state('');
	let input = $state<HTMLInputElement | null>(null);
	let panel = $state<HTMLElement | null>(null);
	let open = $state(false);
	// Position while walking the history with the arrow keys; -1 means "editing a new line"
	let historyIndex = $state(-1);
	const noSend = $derived(sendDisabled ?? disabled);

	// A box that goes dead takes its panel with it, and the half-typed line too:
	// there is nothing left to send it to, so leaving it sitting in a disabled
	// field is just residue — the next connection starts on an empty box
	$effect(() => {
		if (!disabled) return;
		open = false;
		text = '';
		historyIndex = -1;
	});

	function openHistory() {
		if (disabled) return;
		open = true;
	}

	function submit() {
		if (noSend) return;
		if (onsend(text)) recordSend(text);
		text = '';
		historyIndex = -1;
	}

	function onKeydown(e: KeyboardEvent) {
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
		open = true;
		const next = e.key === 'ArrowUp' ? historyIndex + 1 : historyIndex - 1;
		historyIndex = Math.min(Math.max(next, -1), list.length - 1);
		text = historyIndex === -1 ? '' : list[historyIndex];
	}

	/**
	 * The drop-down is a top-layer popover, so it is neither clipped by the tile nor
	 * capped at its width: it takes as much room as its longest line needs, spilling
	 * over the neighbouring tiles, and is only ever pulled back at the window edge.
	 * Nothing positions a popover for us, hence the placing by hand.
	 */
	$effect(() => {
		const el = panel;
		const anchor = input;
		if (!el || !anchor) return;
		// Re-place when the list (and with it the width) changes
		sendHistory.list.length;
		const MARGIN = 6;
		const at = anchor.getBoundingClientRect();
		el.style.minWidth = `${at.width}px`;
		el.style.left = `${at.left}px`;
		el.style.top = `${at.bottom + 4}px`;
		if (!el.matches(':popover-open')) el.showPopover();
		const box = el.getBoundingClientRect();
		const overflowX = box.right - (window.innerWidth - MARGIN);
		if (overflowX > 0) el.style.left = `${Math.max(MARGIN, at.left - overflowX)}px`;
		// No room below: flip above the input
		if (box.bottom > window.innerHeight - MARGIN) {
			el.style.top = `${Math.max(MARGIN, at.top - box.height - 4)}px`;
		}
		return () => {
			if (el.matches(':popover-open')) el.hidePopover();
		};
	});

	// Keep the arrow-key position in view when the list is long enough to scroll
	$effect(() => {
		if (historyIndex < 0 || !panel) return;
		panel.querySelectorAll('.entry')[historyIndex]?.scrollIntoView({ block: 'nearest' });
	});

	/** Clicking a history line loads it for editing; sending stays an explicit step */
	function useHistory(line: string) {
		text = line;
		historyIndex = -1;
		input?.focus();
	}
</script>

<svelte:window
	onresize={() => (open = false)}
	onpointerdowncapture={(e) => {
		// Capture phase: a tile swallows pointerdown over its own send box, which would
		// otherwise leave another tile's panel open
		if (!open) return;
		const target = e.target as Node;
		if (panel?.contains(target) || target === input) return;
		open = false;
	}}
/>

<div class="send-box" class:compact class:fill>
	<input
		type="text"
		class="field"
		{placeholder}
		spellcheck="false"
		{disabled}
		bind:this={input}
		bind:value={text}
		onfocus={openHistory}
		onclick={openHistory}
		onkeydown={onKeydown}
	/>
	{#if !compact}
		<button type="button" class="send" title={t('multi.send')} disabled={noSend} onclick={submit}>
			{t('multi.send')}
		</button>
	{/if}
	<!-- Nothing sent yet means nothing to drop down -->
	{#if open && sendHistory.list.length > 0}
		<div class="panel" popover="manual" bind:this={panel}>
			<ul>
				{#each sendHistory.list as line, i (line)}
					<li>
						<!-- One line each: the panel is sized so none of them has to be cut -->
						<button
							type="button"
							class="entry"
							class:current={i === historyIndex}
							onclick={() => useHistory(line)}
						>
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
	.field {
		width: 250px;
		height: 28px;
		box-sizing: border-box;
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 0 8px;
		font-family: inherit;
		/* One text size for both placements — the compact variant only changes the box */
		font-size: 0.78rem;
		outline: none;
	}
	.field:focus {
		border-color: var(--border-accent);
	}
	.field:disabled {
		opacity: 0.45;
		cursor: default;
	}
	/* In the toolbar the box eats the free width, and the panel follows it */
	.send-box.fill {
		flex: 1;
		min-width: 0;
	}
	/* Sitting in the window title bar, this copy is that bar's omnibox: it takes
	   its tones from the bar rather than from the theme's inputs */
	.fill .field {
		width: 100%;
		min-width: 0;
		background: var(--window-title-input-bg);
		color: var(--window-title-input-fg);
		border-color: var(--window-title-input-border);
	}
	.fill .field::placeholder {
		color: var(--window-title-input-faint);
	}
	.fill .field:focus {
		border-color: var(--window-title-active);
	}
	/* In a title bar the input takes the width it is given, and its tones from
	   that bar rather than from the theme's inputs */
	.compact {
		width: 100%;
	}
	.compact .field {
		width: 100%;
		height: 22px;
		padding: 0 6px;
		background: var(--titlebar-input-bg);
		color: var(--titlebar-input-fg);
		border-color: var(--titlebar-input-border);
	}
	.compact .field::placeholder {
		color: var(--titlebar-input-faint);
	}
	.compact .field:focus {
		border-color: var(--titlebar-active);
	}
	.send {
		/* The widened input must not squeeze the label onto a second line */
		flex-shrink: 0;
		white-space: nowrap;
		/* This button only ever renders in the window title bar — the compact copy
		   in a tile bar has none — so it wears that bar's tones. Primary blue was
		   the one saturated thing on an otherwise grey bar and shouted for it. */
		background: transparent;
		color: var(--window-title-fg);
		border: 1px solid var(--window-title-input-border);
		border-radius: 5px;
		padding: 3px 12px;
		/* Toolbar caption scale — this button only ever shows next to them */
		font-size: 0.7rem;
		cursor: pointer;
	}
	.send:hover:not(:disabled) {
		/* The same fill the toolbar's icon buttons take on hover */
		background: var(--window-title-hover);
	}
	.send:disabled {
		opacity: 0.45;
		cursor: default;
	}
	/* Placed by hand in viewport coordinates, so the UA's centring inset is dropped */
	.panel {
		position: fixed;
		inset: auto;
		margin: 0;
		z-index: 50;
		/* As wide as the longest entry, never narrower than the input it hangs from */
		width: max-content;
		max-width: min(860px, 94vw);
		padding: 0.5rem;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 8px;
		box-shadow: 0 8px 20px var(--shadow);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
		max-height: min(50vh, 320px);
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
	.entry:hover,
	.entry:focus-visible {
		border-color: var(--border-accent);
	}
	/* Where the arrow keys are */
	.entry.current {
		border-color: var(--border-accent);
		color: var(--accent);
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
