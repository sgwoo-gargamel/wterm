<script lang="ts">
	import { onMount } from 'svelte';
	import type { Terminal } from '@xterm/xterm';
	import type { FitAddon } from '@xterm/addon-fit';
	import '@xterm/xterm/css/xterm.css';
	import type { Session } from '$lib/stores/sessions.svelte';
	import { themeState } from '$lib/stores/theme.svelte';
	import { settingsState } from '$lib/stores/settings.svelte';
	import { getTerminal, attachToContainer, xtermTheme } from '$lib/terminals';

	let {
		session,
		active = false,
		onfocused
	}: { session: Session; active?: boolean; onfocused?: () => void } = $props();

	let container: HTMLDivElement;
	let term: Terminal | undefined;
	let fit: FitAddon | undefined;

	// Coalesce refits to one per frame and debounce the backend resize
	// notification — with several tiles a divider drag fires ResizeObserver
	// for every pointer move on every terminal, which gets expensive.
	let fitQueued = false;
	let resizeTimer: ReturnType<typeof setTimeout> | undefined;

	function doFit() {
		if (fitQueued) return;
		fitQueued = true;
		requestAnimationFrame(() => {
			fitQueued = false;
			if (!term || !fit || container.clientWidth === 0 || container.clientHeight === 0) return;
			fit.fit();
			clearTimeout(resizeTimer);
			resizeTimer = setTimeout(() => {
				if (term) session.resize(term.cols, term.rows);
			}, 120);
		});
	}

	onMount(() => {
		// Terminals are cached per session, so remounts (layout changes) keep the buffer
		const bundle = getTerminal(session);
		term = bundle.term;
		fit = bundle.fit;
		attachToContainer(bundle, container);
		doFit();

		const sink = (data: Uint8Array) => term?.write(data);
		session.attach(sink);

		const ro = new ResizeObserver(doFit);
		ro.observe(container);
		term.focus();

		return () => {
			// Do NOT dispose here — the terminal outlives the view (disposed on session close).
			// Nothing in this teardown may throw: it runs inside Svelte's flush.
			try {
				ro.disconnect();
				session.detach(sink);
				clearTimeout(resizeTimer);
			} catch (e) {
				console.error('terminal view cleanup failed', e);
			}
			term = undefined;
			fit = undefined;
		};
	});

	$effect(() => {
		if (active) term?.focus();
	});

	// Re-reads theme mode and custom colors, so both trigger an update
	$effect(() => {
		void themeState.theme;
		void themeState.overrides[themeState.theme];
		const theme = xtermTheme();
		if (term) term.options.theme = theme;
	});

	// Apply font settings live; cell size changes, so refit and notify the backend
	$effect(() => {
		const { family, size, weight } = settingsState.font;
		if (!term) return;
		term.options.fontFamily = family;
		term.options.fontSize = size;
		term.options.fontWeight = weight;
		term.options.fontWeightBold = weight;
		doFit();
	});
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="term-container"
	bind:this={container}
	onpointerdown={() => onfocused?.()}
></div>

<style>
	.term-container {
		width: 100%;
		height: 100%;
		background: var(--bg-panel);
		/* Extra room at the bottom: the fit addon floors the row count against the
		   content height, so the last row can end within a pixel or two of the box.
		   Without the slack a block cursor on the last line touches the pane border
		   and reads as clipped. Right stays 0 — that strip is xterm's scrollbar. */
		padding: 4px 0 12px 4px;
		box-sizing: border-box;
		overflow: hidden;
	}
</style>
