import type { Profile } from '$lib/ipc';
import { getSetting, setSetting } from '$lib/persist';
import {
	layoutState,
	newPaneFor,
	newSplitId,
	setLayout,
	type LayoutNode
} from './layout.svelte';
import { sessions } from './sessions.svelte';

/** A layout stripped down to what survives a restart: shape, sizes and connections */
export type SavedNode =
	| { kind: 'pane'; profile: Profile | null }
	| {
			kind: 'split';
			direction: 'row' | 'column';
			sizes: number[];
			children: SavedNode[];
	  }
	/** Pre-0.1.9 files: binary splits with a first-child ratio */
	| {
			kind: 'split';
			direction: 'row' | 'column';
			ratio: number;
			children: [SavedNode, SavedNode];
	  };

export interface Workspace {
	id: string;
	name: string;
	layout: SavedNode;
}

export const DEFAULT_NAME = 'wterm';

export const workspacesState = $state<{ list: Workspace[]; name: string }>({
	list: [],
	name: DEFAULT_NAME
});

function isSavedNode(node: unknown): node is SavedNode {
	if (!node || typeof node !== 'object') return false;
	const n = node as { kind?: string; children?: unknown; sizes?: unknown; ratio?: unknown };
	if (n.kind === 'pane') return true;
	if (n.kind !== 'split' || !Array.isArray(n.children) || n.children.length < 2) return false;
	const sized = Array.isArray(n.sizes)
		? n.sizes.length === n.children.length && n.sizes.every((s) => typeof s === 'number')
		: typeof n.ratio === 'number' && n.children.length === 2;
	return sized && n.children.every(isSavedNode);
}

export function initWorkspaces() {
	const saved = getSetting('workspaces');
	if (Array.isArray(saved)) {
		workspacesState.list = (saved as Workspace[]).filter(
			(w) => w && typeof w.id === 'string' && typeof w.name === 'string' && isSavedNode(w.layout)
		);
	}
}

function persist() {
	setSetting('workspaces', $state.snapshot(workspacesState.list));
}

/** The label doubles as the current workspace name; it starts over at each launch */
export function setWorkspaceName(name: string) {
	workspacesState.name = name.trim() || DEFAULT_NAME;
}

/** A pane keeps its connection even before it is up, so a mid-restore save is not lossy */
function capture(node: LayoutNode): SavedNode {
	if (node.kind === 'pane') {
		const live = node.sessionId ? sessions.get(node.sessionId)?.profile : undefined;
		const profile = live ?? node.pending ?? null;
		return { kind: 'pane', profile: profile ? ($state.snapshot(profile) as Profile) : null };
	}
	return {
		kind: 'split',
		direction: node.direction,
		sizes: [...node.sizes],
		children: node.children.map(capture)
	};
}

/** Rebuild a saved tree with its exact shape; legacy files carry a first-child ratio instead of sizes */
function build(node: SavedNode): LayoutNode {
	if (node.kind === 'pane') return newPaneFor(node.profile);
	return {
		kind: 'split',
		id: newSplitId(),
		direction: node.direction,
		sizes: 'sizes' in node ? [...node.sizes] : [node.ratio, 1 - node.ratio],
		children: node.children.map(build)
	};
}

/**
 * `wterm` is the placeholder every launch starts with, not a name anyone chose.
 * Saving under it would pile unrelated layouts onto one entry and leave the list
 * headed by a workspace the user never meant to create.
 */
export function isPlaceholderName(name: string): boolean {
	const trimmed = name.trim();
	return trimmed === '' || trimmed.toLowerCase() === DEFAULT_NAME;
}

/** Store the current layout under `name`, replacing a workspace of the same name */
export function saveWorkspace(name: string) {
	if (isPlaceholderName(name)) return;
	const trimmed = name.trim();
	const layout = capture(layoutState.root);
	const existing = workspacesState.list.find((w) => w.name === trimmed);
	if (existing) existing.layout = layout;
	else workspacesState.list.unshift({ id: crypto.randomUUID(), name: trimmed, layout });
	setWorkspaceName(trimmed);
	persist();
}

/** Replace the current layout with the saved one; open sessions are closed */
export function loadWorkspace(id: string) {
	const workspace = workspacesState.list.find((w) => w.id === id);
	if (!workspace) return;
	setLayout(build(workspace.layout));
	setWorkspaceName(workspace.name);
}

/**
 * Back to a launch-fresh state: one empty tile under the placeholder name. The
 * way out of a workspace — without it, deleting the one you are in leaves the
 * label naming something that no longer exists and no route back.
 */
export function newWorkspace() {
	setLayout(newPaneFor(null));
	setWorkspaceName(DEFAULT_NAME);
}

export function removeWorkspace(id: string) {
	const removed = workspacesState.list.find((w) => w.id === id);
	workspacesState.list = workspacesState.list.filter((w) => w.id !== id);
	persist();
	// Deleting the workspace currently open drops its name too; the layout stays,
	// so nothing on screen is lost, but it is no longer a saved workspace
	if (removed && removed.name === workspacesState.name) setWorkspaceName(DEFAULT_NAME);
}
