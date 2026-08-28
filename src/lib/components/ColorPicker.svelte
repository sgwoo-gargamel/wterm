<script lang="ts">
	import { t } from '$lib/i18n.svelte';

	let { value, onpick }: { value: string; onpick: (color: string) => void } = $props();

	let open = $state(false);
	let root = $state<HTMLElement | null>(null);
	let nativeInput = $state<HTMLInputElement | null>(null);

	// Office-style palette: base hues across the top, tints/shades down each column
	const BASES = [
		'#ffffff',
		'#000000',
		'#9aa1ac',
		'#1f3864',
		'#2e75b6',
		'#c55a11',
		'#7f7f7f',
		'#bf9000',
		'#2f5597',
		'#548235'
	];
	/** Positive mixes toward white, negative toward black */
	const LEVELS = [0.6, 0.35, 0, -0.3, -0.55];
	/**
	 * White and black have nowhere left to go in one direction — mixing them by
	 * the levels above lands on the same cell three times over — so their columns
	 * are written out as plain ramps instead. Both still run light at the top and
	 * dark at the bottom like every other column, and neither repeats a tone the
	 * grey columns already carry.
	 */
	const RAMPS: Record<string, string[]> = {
		'#ffffff': ['#ffffff', '#f2f2f2', '#e0e0e0', '#cfcfcf', '#bdbdbd'],
		'#000000': ['#666666', '#4d4d4d', '#333333', '#1a1a1a', '#000000']
	};
	// One row per level, so the grid can be laid out row by row
	const ROWS = LEVELS.map((level, row) =>
		BASES.map((base) => RAMPS[base]?.[row] ?? (level === 0 ? base : mix(base, level)))
	);

	const STANDARD = [
		'#7f1d1d',
		'#dc2626',
		'#f97316',
		'#facc15',
		'#a3e635',
		'#22c55e',
		'#38bdf8',
		'#2563eb',
		'#1e3a8a',
		'#7c3aed'
	];

	function mix(hex: string, amount: number): string {
		const n = parseInt(hex.slice(1), 16);
		const target = amount > 0 ? 255 : 0;
		const k = Math.abs(amount);
		const ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) =>
			Math.round(c + (target - c) * k)
		);
		return `#${ch.map((c) => c.toString(16).padStart(2, '0')).join('')}`;
	}

	function pick(color: string) {
		onpick(color);
		open = false;
	}
</script>

<svelte:window
	onpointerdown={(e) => {
		if (open && root && !root.contains(e.target as Node)) open = false;
	}}
/>

<span class="picker" bind:this={root}>
	<button
		type="button"
		class="swatch"
		style="background: {value}"
		aria-label={t('settings.colors')}
		onclick={() => (open = !open)}
	></button>

	{#if open}
		<div class="popover">
			<span class="section">{t('color.theme')}</span>
			<div class="grid">
				{#each ROWS as row, r (r)}
					{#each row as color, c (c)}
						<button
							type="button"
							class="cell"
							class:selected={color.toLowerCase() === value.toLowerCase()}
							style="background: {color}"
							title={color}
							onclick={() => pick(color)}
						></button>
					{/each}
				{/each}
			</div>
			<span class="section">{t('color.standard')}</span>
			<div class="grid standard">
				{#each STANDARD as color (color)}
					<button
						type="button"
						class="cell"
						class:selected={color.toLowerCase() === value.toLowerCase()}
						style="background: {color}"
						title={color}
						onclick={() => pick(color)}
					></button>
				{/each}
			</div>
			<button type="button" class="more" onclick={() => nativeInput?.click()}>
				{t('color.custom')}
			</button>
			<!-- Native picker for arbitrary colors -->
			<input
				type="color"
				class="native"
				bind:this={nativeInput}
				{value}
				oninput={(e) => onpick(e.currentTarget.value)}
				onchange={() => (open = false)}
			/>
		</div>
	{/if}
</span>

<style>
	.picker {
		position: relative;
		display: inline-flex;
	}
	.swatch {
		width: 40px;
		height: 22px;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 4px;
		cursor: pointer;
	}
	.swatch:hover {
		border-color: var(--border-accent);
	}
	.popover {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		z-index: 60;
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 7px;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 6px;
		box-shadow: 0 8px 18px var(--shadow);
	}
	.section {
		font-size: 0.64rem;
		color: var(--fg-muted);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(10, 15px);
		gap: 2px;
	}
	.cell {
		width: 15px;
		height: 15px;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 2px;
		cursor: pointer;
	}
	.cell:hover {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
	.cell.selected {
		outline: 2px solid var(--active);
		outline-offset: 1px;
	}
	.more {
		margin-top: 2px;
		padding: 3px 0;
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 0.68rem;
		cursor: pointer;
	}
	.more:hover {
		border-color: var(--border-accent);
	}
	.native {
		/* Kept in the DOM only to open the OS colour dialog */
		position: absolute;
		width: 0;
		height: 0;
		opacity: 0;
		pointer-events: none;
	}
</style>
