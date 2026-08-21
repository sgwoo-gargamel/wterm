<script lang="ts">
	import { onMount } from 'svelte';
	import type { Terminal } from '@xterm/xterm';
	import type { FitAddon } from '@xterm/addon-fit';
	import '@xterm/xterm/css/xterm.css';
	import type { Session } from '$lib/stores/sessions.svelte';
	import { themeState } from '$lib/stores/theme.svelte';
	import { settingsState, setFont } from '$lib/stores/settings.svelte';
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

		// Ctrl+wheel zooms the font. Capture phase so xterm's viewport never sees
		// the event (it would scroll the buffer), and non-passive because Svelte's
		// onwheel attribute is passive and couldn't block WebView2's page zoom.
		const onWheel = (e: WheelEvent) => {
			if (!e.ctrlKey) return;
			e.preventDefault();
			e.stopPropagation();
			setFont({ size: settingsState.font.size + (e.deltaY < 0 ? 1 : -1) });
		};
		container.addEventListener('wheel', onWheel, { capture: true, passive: false });

		return () => {
			// Do NOT dispose here — the terminal outlives the view (disposed on session close).
			// Nothing in this teardown may throw: it runs inside Svelte's flush.
			try {
				ro.disconnect();
				container.removeEventListener('wheel', onWheel, { capture: true });
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
		overflow: hidden;
	}
	/* Breathing room around the text on all four sides, so glyphs never touch the
	   tile border or get clipped at the pane edge. The padding MUST live on the
	   .xterm element, not on .term-container: the fit addon subtracts the .xterm
	   element's own padding, but it measures the parent via getComputedStyle()
	   height/width, which Chromium reports as the border-box — parent padding is
	   invisible to it, so rows overshoot and the last line gets cut off. The
	   viewport (scrollbar + background) still spans the full box, so the padding
	   area is painted in the terminal background colour. */
	.term-container :global(.xterm) {
		padding: 4px 4px 6px 6px;
	}
	/* xterm 6 paints the theme background only on the scrollable element (the
	   text area); the viewport behind it keeps xterm.css's static #000 and spans
	   the whole padded box, so it would tint the padding ring black. Let the
	   container's --bg-panel show instead — it tracks the terminal background
	   (built-in themes match it by design, and user overrides update it too). */
	.term-container :global(.xterm-viewport) {
		background: transparent;
	}</style>
