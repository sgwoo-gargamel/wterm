import type { Profile } from '$lib/ipc';
import { closeSession } from './sessions.svelte';

export interface PaneNode {
	kind: 'pane';
	id: string;
	sessionId: string | null;
	/** Connection a restored workspace wants this pane to open once it mounts */
	pending?: Profile | null;
}

export interface SplitNode {
	kind: 'split';
	id: string;
	direction: 'row' | 'column';
	/** Fraction of the split each child takes, one per child, summing to 1 */
	sizes: number[];
	children: LayoutNode[];
}

export type LayoutNode = PaneNode | SplitNode;

let nextId = 1;

function newPane(): PaneNode {
	return { kind: 'pane', id: `pane-${nextId++}`, sessionId: null };
}

const initialPane = newPane();

export const layoutState = $state<{
	root: LayoutNode;
	activePaneId: string;
	/** When set, only this pane is shown, filling the whole tile area */
	maximizedPaneId: string | null;
}>({
	root: initialPane,
	activePaneId: initialPane.id,
	maximizedPaneId: null
});

function findPane(node: LayoutNode, paneId: string): PaneNode | null {
	if (node.kind === 'pane') return node.id === paneId ? node : null;
	for (const child of node.children) {
		const found = findPane(child, paneId);
		if (found) return found;
	}
	return null;
}

function firstPane(node: LayoutNode): PaneNode {
	return node.kind === 'pane' ? node : firstPane(node.children[0]);
}

function equalSizes(count: number): number[] {
	return Array.from({ length: count }, () => 1 / count);
}

/** Scale sizes so they sum to 1 again (after a child was added or removed) */
function normalize(sizes: number[]): number[] {
	const total = sizes.reduce((a, b) => a + b, 0);
	return total > 0 ? sizes.map((s) => s / total) : equalSizes(sizes.length);
}

/**
 * Tree with `paneId` replaced by a split of `count` equal panes (the original
 * first, `fresh` after) laid out along `direction`. The pane always becomes a
 * group of its own, even inside a split of the same direction: with [1|2] on
 * screen, splitting 2 three ways yields [1 | [2|3|4]], so the divider after 1
 * moves the whole group and the dividers inside it move only its members.
 */
function splitInTree(
	node: LayoutNode,
	paneId: string,
	direction: 'row' | 'column',
	fresh: PaneNode[]
): LayoutNode {
	if (node.kind === 'pane') {
		if (node.id !== paneId) return node;
		return {
			kind: 'split',
			id: `split-${nextId++}`,
			direction,
			sizes: equalSizes(fresh.length + 1),
			children: [node, ...fresh]
		};
	}
	return {
		...node,
		children: node.children.map((c) => splitInTree(c, paneId, direction, fresh))
	};
}

/** Tree with the paneId leaf removed and its space handed to its siblings. Null if the root itself is the target */
function removeNode(node: LayoutNode, paneId: string): LayoutNode | null {
	if (node.kind === 'pane') return node.id === paneId ? null : node;
	const children: LayoutNode[] = [];
	const sizes: number[] = [];
	node.children.forEach((child, i) => {
		const kept = removeNode(child, paneId);
		if (kept === null) return;
		children.push(kept);
		sizes.push(node.sizes[i]);
	});
	if (children.length === node.children.length) return { ...node, children };
	if (children.length === 1) return children[0];
	return { ...node, children, sizes: normalize(sizes) };
}

/** Empty pane that connects to `pending` as soon as it is on screen */
export function newPaneFor(pending: Profile | null): PaneNode {
	return { kind: 'pane', id: `pane-${nextId++}`, sessionId: null, pending };
}

export function newSplitId(): string {
	return `split-${nextId++}`;
}

function sessionIds(node: LayoutNode): string[] {
	if (node.kind === 'pane') return node.sessionId ? [node.sessionId] : [];
	return node.children.flatMap(sessionIds);
}

/** Swap in a whole new layout, terminating every session the old one held */
export function setLayout(root: LayoutNode) {
	for (const id of sessionIds(layoutState.root)) closeSession(id);
	layoutState.root = root;
	layoutState.maximizedPaneId = null;
	layoutState.activePaneId = firstPane(root).id;
}

export function activePane(): PaneNode | null {
	return findPane(layoutState.root, layoutState.activePaneId);
}

export function paneById(paneId: string): PaneNode | null {
	return findPane(layoutState.root, paneId);
}

export function countPanes(node: LayoutNode = layoutState.root): number {
	return node.kind === 'pane' ? 1 : node.children.reduce((n, c) => n + countPanes(c), 0);
}

/** Show this pane alone (or restore the full layout when it is already maximized) */
export function toggleMaximize(paneId: string) {
	layoutState.maximizedPaneId = layoutState.maximizedPaneId === paneId ? null : paneId;
	layoutState.activePaneId = paneId;
}

/** Pane currently being dragged for a swap, if any */
export const dragState = $state<{ paneId: string | null }>({ paneId: null });

/** Swap the contents (sessions) of two panes */
export function swapPanes(aId: string, bId: string) {
	const a = findPane(layoutState.root, aId);
	const b = findPane(layoutState.root, bId);
	if (!a || !b || a === b) return;
	const tmp = a.sessionId;
	a.sessionId = b.sessionId;
	b.sessionId = tmp;
	layoutState.activePaneId = bId;
}

export type AddSide = 'left' | 'right' | 'up' | 'down';

/**
 * Split the given pane into `count` equal parts (original keeps the first slot,
 * new panes go right/below). The original pane stays active.
 */
export function splitPane(paneId: string, direction: 'row' | 'column', count = 2) {
	layoutState.maximizedPaneId = null;
	const clamped = Math.max(2, Math.min(4, Math.floor(count)));
	const fresh = Array.from({ length: clamped - 1 }, () => newPane());
	layoutState.root = splitInTree(layoutState.root, paneId, direction, fresh);
	layoutState.activePaneId = paneId;
}

/**
 * Add a new tile on the given side of the WHOLE layout (not a split of one pane):
 * the new tile takes half the window and everything on screen shares the other
 * half. e.g. with [1|2] on screen, adding below yields a full-width 3 underneath;
 * adding right yields a full-height 3 with [1|2] squeezed into the left half.
 */
export function addTile(side: AddSide) {
	// A new tile must be visible, so drop out of maximized view
	layoutState.maximizedPaneId = null;
	const direction = side === 'left' || side === 'right' ? 'row' : 'column';
	const newFirst = side === 'left' || side === 'up';
	const fresh = newPane();
	layoutState.root = {
		kind: 'split',
		id: `split-${nextId++}`,
		direction,
		sizes: [0.5, 0.5],
		children: newFirst ? [fresh, layoutState.root] : [layoutState.root, fresh]
	};
	layoutState.activePaneId = fresh.id;
}

/** Close a pane, terminating its session if any. The last remaining pane resets to empty instead */
export function closePane(paneId: string) {
	const pane = findPane(layoutState.root, paneId);
	if (!pane) return;
	if (pane.sessionId) closeSession(pane.sessionId);
	if (layoutState.maximizedPaneId === paneId) layoutState.maximizedPaneId = null;

	const remaining = removeNode(layoutState.root, paneId);
	if (remaining === null) {
		const fresh = newPane();
		layoutState.root = fresh;
		layoutState.activePaneId = fresh.id;
	} else {
		layoutState.root = remaining;
		if (!findPane(remaining, layoutState.activePaneId)) {
			layoutState.activePaneId = firstPane(remaining).id;
		}
	}
}
