<script lang="ts">
	import { profilesState, removeProfile, type ProfileEntry } from '$lib/stores/profiles.svelte';
	import { portUnavailable } from '$lib/stores/ports.svelte';
	import { t } from '$lib/i18n.svelte';

	function unavailable(entry: ProfileEntry): boolean {
		return entry.profile.type === 'serial' && portUnavailable(entry.profile.port);
	}

	let { onselect }: { onselect: (entry: ProfileEntry) => void } = $props();

	const TYPE_LABEL: Record<string, string> = {
		serial: 'SER',
		ssh: 'SSH',
		telnet: 'TEL',
		local: 'LOC'
	};

	// Display order: ssh → telnet → local → serial. Serial entries sort ascending
	// by port (natural order, so COM8 < COM10); other types keep stored order.
	const TYPE_ORDER: Record<string, number> = { ssh: 0, telnet: 1, local: 2, serial: 3 };
	const sorted = $derived(
		[...profilesState.list].sort((a, b) => {
			const byType = (TYPE_ORDER[a.profile.type] ?? 9) - (TYPE_ORDER[b.profile.type] ?? 9);
			if (byType !== 0) return byType;
			if (a.profile.type === 'serial' && b.profile.type === 'serial') {
				return (
					a.profile.port.localeCompare(b.profile.port, undefined, { numeric: true }) ||
					a.profile.baud_rate - b.profile.baud_rate
				);
			}
			return 0;
		})
	);
</script>

<div class="profile-list">
	<h3>{t('history.title')}</h3>
	{#if profilesState.list.length === 0}
		<p class="empty">{t('history.empty')}</p>
	{:else}
		<ul>
			{#each sorted as entry (entry.id)}
				<li>
					<button
					type="button"
					class="entry"
					class:unavailable={unavailable(entry)}
					onclick={() => onselect(entry)}
				>
						<span class="badge {entry.profile.type}">{TYPE_LABEL[entry.profile.type]}</span>
						<span class="name" title={entry.name}>{entry.name}</span>
					</button>
					<button
						type="button"
						class="del"
						title={t('history.remove')}
						onclick={() => removeProfile(entry.id)}
					>
						✕
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.profile-list {
		min-width: 250px;
		max-width: 280px;
	}
	h3 {
		margin: 0 0 0.45rem;
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--fg-muted);
	}
	.empty {
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
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 0.28rem 0.5rem;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 5px;
		color: var(--fg);
		cursor: pointer;
		text-align: left;
		min-width: 0;
	}
	.entry:hover {
		border-color: var(--border-accent);
	}
	/* Serial entry whose port is absent or held elsewhere */
	.entry.unavailable {
		opacity: 0.45;
	}
	.entry.unavailable .badge {
		filter: grayscale(1);
	}
	.badge {
		font-size: 0.58rem;
		font-weight: 700;
		padding: 1px 4px;
		border-radius: 3px;
		flex-shrink: 0;
	}
	.badge.serial {
		background: var(--badge-serial-bg);
		color: var(--badge-serial-fg);
	}
	.badge.ssh {
		background: var(--badge-ssh-bg);
		color: var(--badge-ssh-fg);
	}
	.badge.telnet {
		background: var(--badge-telnet-bg);
		color: var(--badge-telnet-fg);
	}
	.badge.local {
		background: var(--accent-soft);
		color: var(--accent);
	}
	.name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.78rem;
	}
	.del {
		background: none;
		border: none;
		color: var(--fg-faint);
		cursor: pointer;
		padding: 0 5px;
		font-size: 0.78rem;
		border-radius: 5px;
	}
	.del:hover {
		color: var(--danger);
		background: var(--bg-input);
	}
</style>
