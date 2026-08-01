<script lang="ts">
	import TileNode from './TileNode.svelte';
	import Pane from './Pane.svelte';
	import type { LayoutNode, SplitNode } from '$lib/stores/layout.svelte';

	let { node }: { node: LayoutNode } = $props();

	let container: HTMLDivElement | undefined = $state();

	function startDrag(e: PointerEvent, split: SplitNode) {
		e.preventDefault();
		const rect = container?.getBoundingClientRect();
		if (!rect) return;

		// Throttle ratio updates to one per frame — every update reflows all
		// descendant terminals, which is costly with several tiles
		let raf = 0;
		let lastEvent: PointerEvent | null = null;
		const move = (ev: PointerEvent) => {
			lastEvent = ev;
			if (raf) return;
			raf = requestAnimationFrame(() => {
				raf = 0;
				if (!lastEvent) return;
				const frac =
					split.direction === 'row'
						? (lastEvent.clientX - rect.left) / rect.width
						: (lastEvent.clientY - rect.top) / rect.height;
				split.ratio = Math.min(0.9, Math.max(0.1, frac));
			});
		};
		const up = () => {
			if (raf) cancelAnimationFrame(raf);
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			document.body.style.cursor = '';
			document.body.style.userSelect = '';
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
		document.body.style.cursor = split.direction === 'row' ? 'col-resize' : 'row-resize';
		document.body.style.userSelect = 'none';
	}
</script>

{#if node.kind === 'pane'}
	<Pane pane={node} />
{:else}
	<div class="split {node.direction}" bind:this={container}>
		<div class="child" style="flex-grow: {node.ratio}">
			<TileNode node={node.children[0]} />
		</div>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="divider {node.direction}"
			onpointerdown={(e) => startDrag(e, node)}
			ondblclick={() => (node.ratio = 0.5)}
		></div>
		<div class="child" style="flex-grow: {1 - node.ratio}">
			<TileNode node={node.children[1]} />
		</div>
	</div>
{/if}

<style>
	.split {
		display: flex;
		width: 100%;
		height: 100%;
		min-width: 0;
		min-height: 0;
	}
	.split.row {
		flex-direction: row;
	}
	.split.column {
		flex-direction: column;
	}
	.child {
		flex-basis: 0;
		min-width: 0;
		min-height: 0;
		display: flex;
	}
	.divider {
		flex-shrink: 0;
		background: var(--border);
		transition: background 0.1s;
		z-index: 1;
	}
	.divider:hover {
		background: var(--border-accent);
	}
	.divider.row {
		width: 4px;
		cursor: col-resize;
	}
	.divider.column {
		height: 4px;
		cursor: row-resize;
	}
</style>
