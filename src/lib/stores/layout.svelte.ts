import { closeSession } from './sessions.svelte';

export interface PaneNode {
	kind: 'pane';
	id: string;
	sessionId: string | null;
}

export interface SplitNode {
	kind: 'split';
	id: string;
	direction: 'row' | 'column';
	/** Ratio of the first child (0-1) */
	ratio: number;
	children: [LayoutNode, LayoutNode];
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
	return findPane(node.children[0], paneId) ?? findPane(node.children[1], paneId);
}

function firstPane(node: LayoutNode): PaneNode {
	return node.kind === 'pane' ? node : firstPane(node.children[0]);
}

/** Return a new tree with the paneId leaf replaced by the replacer's result */
function replaceNode(
	node: LayoutNode,
	paneId: string,
	replacer: (pane: PaneNode) => LayoutNode
): LayoutNode {
	if (node.kind === 'pane') return node.id === paneId ? replacer(node) : node;
	return {
		...node,
		children: [
			replaceNode(node.children[0], paneId, replacer),
			replaceNode(node.children[1], paneId, replacer)
		]
	};
}

/** Tree with the paneId leaf removed and its sibling promoted. Null if the root itself is the target */
function removeNode(node: LayoutNode, paneId: string): LayoutNode | null {
	if (node.kind === 'pane') return node.id === paneId ? null : node;
	const [a, b] = node.children;
	const ra = removeNode(a, paneId);
	if (ra === null) return b;
	const rb = removeNode(b, paneId);
	if (rb === null) return a;
	return { ...node, children: [ra, rb] };
}

export function activePane(): PaneNode | null {
	return findPane(layoutState.root, layoutState.activePaneId);
}

export function paneById(paneId: string): PaneNode | null {
	return findPane(layoutState.root, paneId);
}

export function countPanes(node: LayoutNode = layoutState.root): number {
	return node.kind === 'pane'
		? 1
		: countPanes(node.children[0]) + countPanes(node.children[1]);
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
 * new panes go right/below). Built as nested binary splits with 1/n ratios.
 */
export function splitPane(paneId: string, direction: 'row' | 'column', count = 2) {
	layoutState.maximizedPaneId = null;
	const clamped = Math.max(2, Math.min(4, Math.floor(count)));
	const fresh = Array.from({ length: clamped - 1 }, () => newPane());

	layoutState.root = replaceNode(layoutState.root, paneId, (pane) => {
		const nodes: LayoutNode[] = [pane, ...fresh];
		let acc = nodes[nodes.length - 1];
		for (let i = nodes.length - 2; i >= 0; i--) {
			const remaining = nodes.length - i;
			acc = {
				kind: 'split',
				id: `split-${nextId++}`,
				direction,
				ratio: 1 / remaining,
				children: [nodes[i], acc]
			};
		}
		return acc;
	});
	layoutState.activePaneId = fresh[0]?.id ?? paneId;
}

/**
 * Add a new tile on the given side of the WHOLE layout (not a split of one pane).
 * e.g. with [1|2] on screen, adding below yields a full-width 3 underneath.
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
		ratio: 0.5,
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
