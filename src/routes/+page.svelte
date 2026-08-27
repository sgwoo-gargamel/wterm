<script lang="ts">
	import { onMount } from 'svelte';
	import TileNode from '$lib/components/TileNode.svelte';
	import { listFonts, appVersion, type VersionInfo } from '$lib/ipc';
	import { initPersist } from '$lib/persist';
	import Pane from '$lib/components/Pane.svelte';
	import ColorPicker from '$lib/components/ColorPicker.svelte';
	import { layoutState, addTile, closePane, paneById } from '$lib/stores/layout.svelte';
	// One icon rotated four ways, so all four directions read as the same family
	import panelAddIcon from '@fluentui/svg-icons/icons/panel_left_add_20_filled.svg?raw';
	import textFontSizeIcon from '@fluentui/svg-icons/icons/text_font_size_20_regular.svg?raw';
	import folderIcon from '@fluentui/svg-icons/icons/folder_20_regular.svg?raw';
	import questionIcon from '@fluentui/svg-icons/icons/question_circle_20_regular.svg?raw';
	import { open } from '@tauri-apps/plugin-dialog';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import {
		selectAll,
		clearTargets,
		allSelected,
		sendToTargets,
		connectedCount,
		targetCount
	} from '$lib/stores/multisend.svelte';
	import { initSendHistory } from '$lib/stores/sendhistory.svelte';
	import {
		workspacesState,
		initWorkspaces,
		saveWorkspace,
		loadWorkspace,
		removeWorkspace,
		setWorkspaceName,
		isPlaceholderName,
		newWorkspace
	} from '$lib/stores/workspaces.svelte';
	import SendBox from '$lib/components/SendBox.svelte';
	import chevronDownIcon from '@fluentui/svg-icons/icons/chevron_down_20_regular.svg?raw';
	import { loadProfiles } from '$lib/stores/profiles.svelte';
	import { i18n, initLocale, setLocale, t, type MessageKey } from '$lib/i18n.svelte';
	import {
		themeState,
		initTheme,
		toggleTheme,
		setColor,
		resetColors,
		type ColorKey
	} from '$lib/stores/theme.svelte';
	import { XTERM_THEMES, applyRendererToAll } from '$lib/terminals';
	import { saveStatus, clearSaveFailure } from '$lib/stores/savestatus.svelte';

	// Colour pickers show the effective value, so unset fields start from the built-in theme
	const COLOR_FIELDS: { key: ColorKey; label: MessageKey; fallback: () => string }[] = [
		{
			key: 'background',
			label: 'settings.colorBg',
			fallback: () => XTERM_THEMES[themeState.theme].background ?? '#000000'
		},
		{
			key: 'foreground',
			label: 'settings.colorFg',
			fallback: () => XTERM_THEMES[themeState.theme].foreground ?? '#ffffff'
		},
		{
			key: 'cursor',
			label: 'settings.colorCursor',
			fallback: () => XTERM_THEMES[themeState.theme].cursor ?? '#ffffff'
		},
		{
			key: 'accent',
			label: 'settings.colorAccent',
			fallback: () => (themeState.theme === 'dark' ? '#6ea3ff' : '#2563eb')
		}
	];
	import {
		settingsState,
		initSettings,
		setFont,
		resetFont,
		setRenderer,
		setLogDir,
		type RendererKind
	} from '$lib/stores/settings.svelte';

	// Maximized tile takes over the whole area; null renders the normal layout
	const maximizedPane = $derived(
		layoutState.maximizedPaneId ? paneById(layoutState.maximizedPaneId) : null
	);

	async function pickLogDir() {
		const dir = await open({
			directory: true,
			multiple: false,
			defaultPath: settingsState.logDir || undefined
		});
		if (typeof dir === 'string') setLogDir(dir);
	}

	let settingsOpen = $state(false);
	let settingsPanel = $state<HTMLElement | null>(null);
	let settingsButton = $state<HTMLElement | null>(null);

	// --- Workspaces: the label names the current one, the caret opens the list ---
	let workspaceOpen = $state(false);
	let workspacePanel = $state<HTMLElement | null>(null);
	let workspaceButton = $state<HTMLElement | null>(null);

	// System monospace fonts, loaded from the backend when the panel opens
	let fonts = $state<string[]>([]);
	const currentFontName = $derived(
		fonts.find((n) => settingsState.font.family.startsWith(`"${n}"`)) ?? null
	);
	/** First family name of the stored string, for display when it is not in the list */
	const storedFontLabel = $derived(
		settingsState.font.family.split(',')[0].replace(/"/g, '').trim()
	);
	const FONT_WEIGHTS: { value: number; key: 'light' | 'normal' | 'medium' | 'bold' }[] = [
		{ value: 300, key: 'light' },
		{ value: 400, key: 'normal' },
		{ value: 500, key: 'medium' },
		{ value: 700, key: 'bold' }
	];

	function toggleSettings() {
		settingsOpen = !settingsOpen;
		if (settingsOpen && fonts.length === 0) {
			void listFonts().then((list) => (fonts = list));
		}
	}

	// --- About: the "?" button opens version + git revision, once the title bar's job ---
	let version = $state<VersionInfo | null>(null);
	let aboutOpen = $state(false);
	let aboutPanel = $state<HTMLElement | null>(null);
	let aboutButton = $state<HTMLElement | null>(null);

	// The window has no native title bar, so the toolbar's empty space acts as one:
	// drag to move, double-click to maximize. Only blank toolbar areas qualify, so the
	// buttons, inputs and pop-up panels inside it keep their normal behaviour.
	const appWindow = getCurrentWindow();
	function isDragArea(target: EventTarget | null) {
		const el = target as HTMLElement | null;
		return (
			!!el &&
			(el.classList.contains('toolbar') ||
				el.classList.contains('toolbar-body') ||
				el.classList.contains('group') ||
				el.classList.contains('group-label'))
		);
	}
	function onToolbarMouseDown(e: MouseEvent) {
		if (e.button !== 0 || e.detail !== 1 || !isDragArea(e.target)) return;
		void appWindow.startDragging();
	}
	function onToolbarDblClick(e: MouseEvent) {
		if (!isDragArea(e.target)) return;
		void appWindow.toggleMaximize();
	}

	onMount(() => {
		// Settings live in one JSON file; load it before the stores read from it
		void initPersist().then(() => {
			initLocale();
			initTheme();
			initSettings();
			loadProfiles();
			initSendHistory();
			initWorkspaces();
		});
		void appVersion().then((v) => (version = v));

		// Handle shortcuts in the capture phase before xterm consumes the keys
		const onKeydown = (e: KeyboardEvent) => {
			if (!e.ctrlKey || !e.shiftKey || e.altKey) return;
			const key = e.key.toUpperCase();
			if (key === 'D') {
				e.preventDefault();
				e.stopPropagation();
				addTile('right');
			} else if (key === 'E') {
				e.preventDefault();
				e.stopPropagation();
				addTile('down');
			} else if (key === 'W') {
				e.preventDefault();
				e.stopPropagation();
				closePane(layoutState.activePaneId);
			}
		};
		window.addEventListener('keydown', onKeydown, { capture: true });

		// Close the pop-up panels when clicking outside of them
		const onPointerDown = (e: PointerEvent) => {
			const target = e.target as Node;
			if (settingsOpen && !settingsPanel?.contains(target) && !settingsButton?.contains(target)) {
				settingsOpen = false;
			}
			if (
				workspaceOpen &&
				!workspacePanel?.contains(target) &&
				!workspaceButton?.contains(target)
			) {
				workspaceOpen = false;
			}
			if (aboutOpen && !aboutPanel?.contains(target) && !aboutButton?.contains(target)) {
				aboutOpen = false;
			}
		};
		window.addEventListener('pointerdown', onPointerDown);

		return () => {
			window.removeEventListener('keydown', onKeydown, { capture: true });
			window.removeEventListener('pointerdown', onPointerDown);
		};
	});
</script>

<div class="app">
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<header class="toolbar" onmousedown={onToolbarMouseDown} ondblclick={onToolbarDblClick}>
		<!-- When the window is too narrow the lowest-priority groups are hidden outright
		     (see the container queries below) so nothing overflows into its neighbour;
		     the window controls always stay at the right edge -->
		<div class="toolbar-body">
		<!-- The label is the current workspace name; the caret opens the saved list -->
		<div class="workspace">
			<!-- Keyed so loading a workspace refreshes the text the user can edit -->
			{#key workspacesState.name}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<span
					class="logo"
					role="textbox"
					tabindex="0"
					title={t('workspace.name')}
					contenteditable="plaintext-only"
					spellcheck="false"
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.preventDefault();
							(e.currentTarget as HTMLElement).blur();
						}
					}}
					onblur={(e) => setWorkspaceName(e.currentTarget.textContent ?? '')}
				>{workspacesState.name}</span>
			{/key}
			<button
				type="button"
				class="lang icon-btn caret"
				title={t('workspace.menu')}
				bind:this={workspaceButton}
				onclick={() => (workspaceOpen = !workspaceOpen)}
			>
				{@html chevronDownIcon}
			</button>
			{#if workspaceOpen}
				<div class="workspace-panel" bind:this={workspacePanel}>
					<button
						type="button"
						class="ws-save"
						disabled={isPlaceholderName(workspacesState.name)}
						title={isPlaceholderName(workspacesState.name)
							? t('workspace.needName')
							: t('workspace.hint')}
						onclick={() => {
							saveWorkspace(workspacesState.name);
							workspaceOpen = false;
						}}
					>
						{t('workspace.save')}
					</button>
					<!-- The way back to a launch-fresh tile, and out of a workspace -->
					<button
						type="button"
						class="ws-new"
						title={t('workspace.newHint')}
						onclick={() => {
							newWorkspace();
							workspaceOpen = false;
						}}
					>
						{t('workspace.new')}
					</button>
					{#if isPlaceholderName(workspacesState.name)}
						<p class="ws-need-name">{t('workspace.needName')}</p>
					{/if}
					<span class="panel-section">{t('workspace.saved')}</span>
					{#if workspacesState.list.length === 0}
						<p class="ws-empty">{t('workspace.empty')}</p>
					{:else}
						<ul class="ws-list">
							{#each workspacesState.list as workspace (workspace.id)}
								<li>
									<button
										type="button"
										class="ws-entry"
										class:current={workspace.name === workspacesState.name}
										onclick={() => {
											loadWorkspace(workspace.id);
											workspaceOpen = false;
										}}
									>
										{workspace.name}
									</button>
									<button
										type="button"
										class="ws-del"
										title={t('workspace.remove')}
										onclick={() => removeWorkspace(workspace.id)}
									>
										✕
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			{/if}
		</div>
		<!-- Multi-send: input goes to every tile checked in its title bar.
		     This group takes the toolbar's leftover width so the input is as wide as it gets -->
		<div class="group grow">
			<SendBox
				placeholder="{t('multi.placeholder')} ({targetCount()})"
				disabled={connectedCount() === 0}
				sendDisabled={targetCount() === 0}
				fill
				onsend={(text) => sendToTargets(text) > 0}
			/>
			<label class="multi-check" title={t('multi.all')}>
				<input
					type="checkbox"
					checked={allSelected()}
					disabled={connectedCount() === 0}
					onchange={(e) => (e.currentTarget.checked ? selectAll() : clearTargets())}
				/>
				<span>{t('multi.all')}</span>
			</label>
		</div>

		<!-- Add a tile on each side of the whole layout -->
		<div class="group g-add">
			<span class="group-label">{t('group.addTile')}</span>
			<button
				type="button"
				class="lang icon-btn"
				title={t('toolbar.addLeft')}
				onclick={() => addTile('left')}
			>
				{@html panelAddIcon}
			</button>
			<button
				type="button"
				class="lang icon-btn rot-180"
				title="{t('toolbar.addRight')} (Ctrl+Shift+D)"
				onclick={() => addTile('right')}
			>
				{@html panelAddIcon}
			</button>
			<button
				type="button"
				class="lang icon-btn rot-90"
				title={t('toolbar.addUp')}
				onclick={() => addTile('up')}
			>
				{@html panelAddIcon}
			</button>
			<button
				type="button"
				class="lang icon-btn rot-270"
				title="{t('toolbar.addDown')} (Ctrl+Shift+E)"
				onclick={() => addTile('down')}
			>
				{@html panelAddIcon}
			</button>
		</div>

		<div class="group g-logdir">
			<span class="group-label">{t('group.logDir')}</span>
			<button
				type="button"
				class="lang icon-btn"
				title="{t('toolbar.logDir')}{settingsState.logDir ? `: ${settingsState.logDir}` : ''}"
				class:configured={!!settingsState.logDir}
				onclick={pickLogDir}
			>
				{@html folderIcon}
			</button>
		</div>

		<div class="group g-settings">
			<span class="group-label">{t('group.settings')}</span>
			<!-- Windows IME-style language toggle: 가 = Korean, A = English -->
			<button
				type="button"
				class="lang"
				title={i18n.locale === 'ko' ? '한국어 → English' : 'English → 한국어'}
				onclick={() => setLocale(i18n.locale === 'ko' ? 'en' : 'ko')}
			>
				{i18n.locale === 'ko' ? '가' : 'A'}
			</button>
			<button
				type="button"
				class="lang icon-btn"
				title={t('toolbar.settings')}
				bind:this={settingsButton}
				onclick={toggleSettings}
			>
				{@html textFontSizeIcon}
			</button>
			<!-- Theme toggle: shows the current theme (☾ dark / ☀ light) -->
			<button type="button" class="lang" title={t('toolbar.theme')} onclick={toggleTheme}>
				{themeState.theme === 'dark' ? '☾' : '☀'}
			</button>
			<button
				type="button"
				class="lang icon-btn"
				title={t('toolbar.about')}
				bind:this={aboutButton}
				onclick={() => (aboutOpen = !aboutOpen)}
			>
				{@html questionIcon}
			</button>
		</div>
		</div>

		<!-- Window controls replace the removed native title bar -->
		<div class="win-controls">
			<button
				type="button"
				title={t('window.minimize')}
				aria-label={t('window.minimize')}
				onclick={() => appWindow.minimize()}
			>
				<svg viewBox="0 0 10 10"><path d="M0 5h10" /></svg>
			</button>
			<button
				type="button"
				title={t('window.maximize')}
				aria-label={t('window.maximize')}
				onclick={() => appWindow.toggleMaximize()}
			>
				<svg viewBox="0 0 10 10"><path d="M0.5 0.5h9v9h-9z" /></svg>
			</button>
			<button
				type="button"
				class="win-close"
				title={t('window.close')}
				aria-label={t('window.close')}
				onclick={() => appWindow.close()}
			>
				<svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" /></svg>
			</button>
		</div>
		{#if settingsOpen}
			<div class="settings-panel" bind:this={settingsPanel}>
				<label>
					<span>{t('settings.fontFamily')}</span>
					<select
						value={currentFontName ?? '__stored__'}
						onchange={(e) => setFont({ family: `"${e.currentTarget.value}", monospace` })}
					>
						{#if !currentFontName}
							<option value="__stored__" disabled>{storedFontLabel}</option>
						{/if}
						{#each fonts as name (name)}
							<option value={name}>{name}</option>
						{/each}
					</select>
				</label>
				<label>
					<span>{t('settings.fontSize')}</span>
					<input
						type="number"
						min="8"
						max="40"
						value={settingsState.font.size}
						onchange={(e) => setFont({ size: Number(e.currentTarget.value) })}
					/>
				</label>
				<label>
					<span>{t('settings.fontWeight')}</span>
					<select
						value={settingsState.font.weight}
						onchange={(e) => setFont({ weight: Number(e.currentTarget.value) })}
					>
						{#each FONT_WEIGHTS as w (w.value)}
							<option value={w.value}>{t(`settings.weight.${w.key}`)}</option>
						{/each}
					</select>
				</label>
				<label>
					<span>{t('settings.renderer')}</span>
					<select
						value={settingsState.renderer}
						onchange={(e) => {
							setRenderer(e.currentTarget.value as RendererKind);
							applyRendererToAll();
						}}
					>
						<option value="webgl">{t('settings.renderer.webgl')}</option>
						<option value="dom">{t('settings.renderer.dom')}</option>
					</select>
				</label>
				<span class="panel-section">{t('settings.colors')}</span>
				{#each COLOR_FIELDS as field (field.key)}
					<div class="color-row">
						<span>{t(field.label)}</span>
						<ColorPicker
							value={themeState.overrides[themeState.theme][field.key] ?? field.fallback()}
							onpick={(color) => setColor(field.key, color)}
						/>
					</div>
				{/each}
				<div class="panel-buttons">
					<button type="button" class="reset" onclick={resetColors}>
						{t('settings.resetColors')}
					</button>
					<button type="button" class="reset" onclick={resetFont}>{t('settings.reset')}</button>
				</div>
			</div>
		{/if}
		{#if aboutOpen}
			<div class="about-panel" bind:this={aboutPanel}>
				<strong>wterm</strong>
				<div class="about-row">
					<span>{t('app.version')}</span>
					<span>{version ? `v${version.version}` : '…'}</span>
				</div>
				<div class="about-row">
					<span>{t('app.revision')}</span>
					<span class="mono">{version?.rev || '—'}</span>
				</div>
			</div>
		{/if}
	</header>
	<main class="tiles">
		{#if maximizedPane}
			<Pane pane={maximizedPane} />
		{:else}
			<TileNode node={layoutState.root} />
		{/if}
	</main>
</div>

{#if saveStatus.failed}
	<div
		class="save-backdrop"
		role="alertdialog"
		aria-modal="true"
		aria-labelledby="save-error-title"
		tabindex="-1"
		onkeydown={(e) => {
			if (e.key === 'Escape') clearSaveFailure();
		}}
	>
		<div class="save-popup">
			<strong id="save-error-title">{t('save.failed')}</strong>
			<p>{t('save.failedHint')}</p>
			{#if saveStatus.path}<code>{saveStatus.path}</code>{/if}
			{#if saveStatus.detail}<p class="detail">{saveStatus.detail}</p>{/if}
			<!-- Autofocused so Enter and Escape both dismiss without reaching for the mouse -->
			<!-- svelte-ignore a11y_autofocus -->
			<button type="button" autofocus onclick={clearSaveFailure}>{t('save.dismiss')}</button>
		</div>
	</div>
{/if}

<style>
	.save-backdrop {
		position: fixed;
		inset: 0;
		/* Above every in-app layer, including the colour picker at 60 */
		z-index: 100;
		background: var(--overlay);
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.save-popup {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: 30rem;
		padding: 1.2rem 1.4rem;
		background: var(--bg-panel);
		border: 1px solid var(--danger-border);
		border-radius: 8px;
		box-shadow: 0 12px 32px rgb(0 0 0 / 45%);
		color: var(--fg);
		font-size: 0.85rem;
	}
	.save-popup strong {
		color: var(--danger);
		font-size: 0.95rem;
	}
	.save-popup p {
		margin: 0;
	}
	.save-popup code {
		padding: 0.35rem 0.5rem;
		background: var(--bg-input);
		border-radius: 4px;
		font-size: 0.78rem;
		color: var(--fg-muted);
		word-break: break-all;
	}
	.save-popup .detail {
		font-size: 0.78rem;
		color: var(--fg-faint);
		word-break: break-all;
	}
	.save-popup button {
		align-self: flex-end;
		margin-top: 0.3rem;
		padding: 0.35rem 1rem;
		background: var(--accent-soft);
		border: 1px solid var(--border-accent);
		border-radius: 6px;
		color: var(--fg);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.save-popup button:hover {
		background: var(--accent-soft-hover);
	}

	:global(:root) {
		/* Native controls (number spinners, select popups, scrollbars) follow the theme */
		color-scheme: dark;
		--bg: #0f1116;
		--bg-panel: #14161c;
		--bg-elev: #1a1d25;
		--bg-input: #1d2129;
		--accent-soft: #26304a;
		--accent-soft-hover: #2f3c5e;
		--border: #2a2e38;
		--border-accent: #3b558e;
		--fg: #d8dce4;
		--fg-muted: #8b93a1;
		--fg-faint: #5c6370;
		/* Chrome icons/captions: clearly legible but a step below body text */
		--fg-icon: #b4bac6;
		--accent: #6ea3ff;
		--primary: #2b64d9;
		--primary-hover: #3873ee;
		--primary-fg: #ffffff;
		--danger: #e06c75;
		--danger-bg: #3a2226;
		--danger-border: #6e3038;
		--danger-bg-hover: #4a2a30;
		--ok: #4cc373;
		--active: #c9a338;
		--overlay: rgba(10, 12, 16, 0.72);
		--shadow: rgba(0, 0, 0, 0.45);
		--badge-serial-bg: #274427;
		--badge-serial-fg: #7ed491;
		--badge-ssh-bg: #26304a;
		--badge-ssh-fg: #8ab0f5;
		--badge-telnet-bg: #46392a;
		--badge-telnet-fg: #e0b370;
		/* Connection-progress popups (Pane / ConnectForm) keep the dark look in
		   both themes so the hamster wheel reads the same everywhere — the light
		   block below deliberately does not override these */
		--popup-bg: #1a1d25;
		--popup-border: #3b558e;
		--popup-fg: #d8dce4;
		--popup-fg-muted: #8b93a1;
		--popup-fg-faint: #5c6370;
		--popup-danger: #e06c75;
	}
	:global(:root[data-theme='light']) {
		color-scheme: light;
		/* Windows 11 light palette, except the panel: neutral gray to cut the
		   glare of near-white terminal areas. Keep in sync with the light xterm
		   theme background in terminals.ts. */
		--bg: #f3f3f3;
		--bg-panel: #f0efed;
		--bg-elev: #efefef;
		--bg-input: #fbfbfb;
		--accent-soft: #dceafb;
		--accent-soft-hover: #cee0fa;
		--border: #d9d9d9;
		--border-accent: #7fa3e8;
		--fg: #1b1b1b;
		--fg-muted: #5d5d5d;
		--fg-faint: #8a8a8a;
		--fg-icon: #3a3a3a;
		--accent: #2563eb;
		--primary: #2b64d9;
		--primary-hover: #3873ee;
		--primary-fg: #ffffff;
		--danger: #d33f49;
		--danger-bg: #fbe4e6;
		--danger-border: #e8a2a8;
		--danger-bg-hover: #f6d2d6;
		--ok: #0a7d33;
		--active: #b3861a;
		--overlay: rgba(238, 241, 246, 0.78);
		--shadow: rgba(30, 40, 60, 0.2);
		--badge-serial-bg: #dff2e2;
		--badge-serial-fg: #12722d;
		--badge-ssh-bg: #dde8fb;
		--badge-ssh-fg: #1d4fb8;
		--badge-telnet-bg: #f5e8d4;
		--badge-telnet-fg: #8a5b16;
	}
	:global(html, body) {
		margin: 0;
		padding: 0;
		height: 100%;
		background: var(--bg);
		overflow: hidden;
	}
	/* Tile swap animation speed (View Transitions API) */
	:global(::view-transition-group(*)),
	:global(::view-transition-old(*)),
	:global(::view-transition-new(*)) {
		animation-duration: 220ms;
	}
	:global(body) {
		font-family: 'Segoe UI', 'Malgun Gothic', sans-serif;
		color: var(--fg);
	}
	.app {
		display: flex;
		flex-direction: column;
		height: 100vh;
	}
	.toolbar {
		display: flex;
		align-items: center;
		/* Flush on all sides — no frame, no border, no shadow */
		padding: 4px 12px;
		margin: 0;
		background: var(--bg-elev);
		border: none;
		flex-shrink: 0;
	}
	.toolbar-body {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		/* Lets the queries below react to the space left over next to the window controls */
		container-type: inline-size;
	}
	/* Drop groups one by one as the room runs out, least important first. The thresholds
	   are the summed natural widths of what remains, with slack for the longer English labels */
	@container (max-width: 900px) {
		.group.g-logdir {
			display: none;
		}
	}
	@container (max-width: 750px) {
		.group.g-settings {
			display: none;
		}
	}
	@container (max-width: 540px) {
		.group.g-add {
			display: none;
		}
	}
	@container (max-width: 340px) {
		.group.grow {
			display: none;
		}
	}
	/* Anchor for the workspace drop-down */
	.workspace {
		position: relative;
		display: flex;
		align-items: center;
		gap: 2px;
		margin-right: 8px;
	}
	.logo {
		font-weight: 700;
		font-size: 1rem;
		color: var(--accent);
		outline: none;
		min-width: 40px;
		max-width: 220px;
		overflow: hidden;
		white-space: nowrap;
		padding: 2px 4px;
		border-radius: 4px;
		cursor: text;
	}
	.logo:focus {
		background: var(--bg-input);
		box-shadow: 0 0 0 1px var(--border-accent);
	}
	.caret :global(svg) {
		width: 16px;
		height: 16px;
	}
	.workspace-panel {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		z-index: 50;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		width: 240px;
		padding: 0.6rem;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 8px;
		box-shadow: 0 8px 20px var(--shadow);
	}
	.workspace-panel .ws-save {
		padding: 0.3rem 0;
		background: var(--primary);
		color: var(--primary-fg);
		border: 1px solid var(--primary);
		border-radius: 5px;
		font-size: 0.78rem;
		cursor: pointer;
	}
	.workspace-panel .ws-save:hover:not(:disabled) {
		background: var(--primary-hover);
		border-color: var(--primary-hover);
	}
	.workspace-panel .ws-save:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.workspace-panel .ws-new {
		margin-top: 4px;
		padding: 0.3rem 0;
		background: transparent;
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 5px;
		font-size: 0.78rem;
		cursor: pointer;
	}
	.workspace-panel .ws-new:hover {
		border-color: var(--border-accent);
		background: var(--bg-input);
	}
	.ws-need-name {
		margin: 0.35rem 0 0;
		font-size: 0.7rem;
		color: var(--fg-faint);
	}
	.ws-empty {
		margin: 0;
		font-size: 0.74rem;
		color: var(--fg-faint);
	}
	.ws-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
		max-height: 260px;
		overflow-y: auto;
	}
	.ws-list li {
		display: flex;
		gap: 3px;
		align-items: stretch;
	}
	.workspace-panel .ws-entry {
		flex: 1;
		min-width: 0;
		padding: 0.28rem 0.5rem;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 5px;
		color: var(--fg);
		font-size: 0.78rem;
		text-align: left;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		cursor: pointer;
	}
	.workspace-panel .ws-entry:hover {
		border-color: var(--border-accent);
		background: var(--bg-input);
		color: var(--fg);
	}
	/* The one whose name the label carries */
	.workspace-panel .ws-entry.current {
		color: var(--accent);
		border-color: var(--border-accent);
	}
	.workspace-panel .ws-del {
		color: var(--fg-faint);
		padding: 0 5px;
		font-size: 0.78rem;
	}
	.workspace-panel .ws-del:hover {
		color: var(--danger);
		background: var(--bg-input);
	}
	/* Toolbar groups: a caption plus that feature's controls, outlined together */
	.group {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px;
		margin-left: 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		flex-shrink: 0;
	}
	/* The one group that stretches; the rest keep their natural width */
	.group.grow {
		flex: 1;
		/* Never squeeze the send box to nothing — the queries above hide it first */
		min-width: 200px;
		margin-left: 0;
	}
	.group-label {
		/* Same size as the tile title bar's "multi" caption so both toolbars read alike */
		font-size: 0.7rem;
		/* Same tone as the icons so the whole group reads as one unit */
		color: var(--fg-icon);
		margin-right: 2px;
		white-space: nowrap;
	}
	.multi-check {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 0.7rem;
		color: var(--fg-icon);
		cursor: pointer;
		white-space: nowrap;
	}
	.multi-check input {
		margin: 0;
		cursor: pointer;
	}
	.multi-check:hover {
		color: var(--accent);
	}
	/* Borderless controls sized like the tile title-bar buttons */
	.toolbar button {
		background: none;
		/* One tone for every toolbar control, matching the group captions */
		color: var(--fg-icon);
		border: none;
		border-radius: 4px;
		padding: 2px 6px;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.toolbar button:hover {
		background: var(--accent-soft);
		color: var(--accent);
	}
	.lang {
		width: 30px;
		height: 28px;
		padding: 0;
		font-size: 1rem;
		font-weight: 600;
		line-height: 1;
		outline: none;
	}
	.icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
	}
	.icon-btn.configured {
		color: var(--ok);
	}
	.icon-btn :global(svg) {
		/* Same glyph size as the tile title-bar icons */
		width: 20px;
		height: 20px;
		fill: currentColor;
		display: block;
	}
	/* Caption-style window buttons (minimize / maximize / close) at the toolbar's right edge */
	.win-controls {
		display: flex;
		flex-shrink: 0;
		align-self: stretch;
		margin: -4px -12px -4px 4px;
	}
	.win-controls button {
		width: 46px;
		border-radius: 0;
		padding: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.win-controls svg {
		width: 10px;
		height: 10px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1;
	}
	.win-controls .win-close:hover {
		background: #c42b1c;
		color: #fff;
	}
	/* Rotations turn the "add panel on the left" glyph into the other directions */
	.rot-90 :global(svg) {
		transform: rotate(90deg);
	}
	.rot-180 :global(svg) {
		transform: rotate(180deg);
	}
	.rot-270 :global(svg) {
		transform: rotate(270deg);
	}
	.tiles {
		flex: 1;
		min-height: 0;
		/* No top padding so the first tile sits directly under the toolbar. The gap to
		   the window edge stays tighter than the tile's own inner padding, so the
		   terminal content reads as inset from its tile rather than the tile from the app */
		padding: 0 2px 2px;
		display: flex;
	}
	.toolbar {
		position: relative;
	}
	.settings-panel {
		position: absolute;
		top: calc(100% + 4px);
		right: 10px;
		z-index: 50;
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
		width: 220px;
		padding: 0.6rem;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 8px;
		box-shadow: 0 8px 20px var(--shadow);
	}
	/* Same card as the settings panel, sized to its two lines */
	.about-panel {
		position: absolute;
		top: calc(100% + 4px);
		right: 10px;
		z-index: 50;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		min-width: 180px;
		padding: 0.6rem 0.7rem;
		font-size: 0.78rem;
		background: var(--bg-elev);
		border: 1px solid var(--border-accent);
		border-radius: 8px;
		box-shadow: 0 8px 20px var(--shadow);
	}
	.about-panel strong {
		color: var(--accent);
		margin-bottom: 0.15rem;
	}
	.about-row {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
	}
	.about-row span:first-child {
		color: var(--fg-muted);
	}
	.about-row .mono {
		font-family: monospace;
	}
	.settings-panel label {
		display: flex;
		flex-direction: column;
		gap: 0.18rem;
		font-size: 0.7rem;
		color: var(--fg-muted);
	}
	.settings-panel input,
	.settings-panel select {
		background: var(--bg-input);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 0.22rem 0.45rem;
		font-size: 0.76rem;
		height: 25px;
		box-sizing: border-box;
		outline: none;
	}
	.settings-panel input:focus,
	.settings-panel select:focus {
		border-color: var(--border-accent);
	}
	.panel-section {
		margin-top: 0.15rem;
		font-size: 0.68rem;
		font-weight: 600;
		color: var(--fg-muted);
		border-top: 1px solid var(--border);
		padding-top: 0.4rem;
	}
	.color-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		font-size: 0.7rem;
		color: var(--fg-muted);
	}
	.panel-buttons {
		display: flex;
		justify-content: flex-end;
		gap: 5px;
		margin-top: 0.2rem;
	}
	.settings-panel .reset {
		background: var(--bg-input);
		color: var(--fg-muted);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 2px 10px;
		font-size: 0.72rem;
		cursor: pointer;
	}
	.settings-panel .reset:hover {
		color: var(--fg);
		border-color: var(--border-accent);
	}
</style>
