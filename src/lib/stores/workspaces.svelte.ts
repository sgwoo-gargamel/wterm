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

/** A layout stripped down to what survives a restart: shape, ratios and connections */
export type SavedNode =
	| { kind: 'pane'; profile: Profile | null }
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
	const n = node as { kind?: string; children?: unknown };
	if (n.kind === 'pane') return true;
	if (n.kind !== 'split' || !Array.isArray(n.children) || n.children.length !== 2) return false;
	return isSavedNode(n.children[0]) && isSavedNode(n.children[1]);
}

export function initWorkspaces() {
	const saved = getSetting('workspaces');
	if (Array.isArray(saved)) {
		workspacesState.list = (saved as Workspace[]).filter(
			(w) => w && typeof w.id === 'string' && typeof w.name === 'string' && isSavedNode(w.layout)
		);
	}
	workspacesState.name = getSetting('workspace_name')?.trim() || DEFAULT_NAME;
}

function persist() {
	setSetting('workspaces', $state.snapshot(workspacesState.list));
}

/** The label doubles as the current workspace name, so it is remembered too */
export function setWorkspaceName(name: string) {
	workspacesState.name = name.trim() || DEFAULT_NAME;
	setSetting('workspace_name', workspacesState.name);
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
		ratio: node.ratio,
		children: [capture(node.children[0]), capture(node.children[1])]
	};
}

function build(node: SavedNode): LayoutNode {
	if (node.kind === 'pane') return newPaneFor(node.profile);
	return {
		kind: 'split',
		id: newSplitId(),
		direction: node.direction,
		ratio: node.ratio,
		children: [build(node.children[0]), build(node.children[1])]
	};
}

/** Store the current layout under `name`, replacing a workspace of the same name */
export function saveWorkspace(name: string) {
	const trimmed = name.trim() || DEFAULT_NAME;
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

export function removeWorkspace(id: string) {
	workspacesState.list = workspacesState.list.filter((w) => w.id !== id);
	persist();
}
