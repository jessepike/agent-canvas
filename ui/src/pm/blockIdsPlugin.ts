import type { Node as ProseMirrorNode } from "prosemirror-model";
import { Plugin, PluginKey, type EditorState, type Transaction } from "prosemirror-state";
import { Decoration, DecorationSet, type EditorView } from "prosemirror-view";

export type NodeId = string;
export type BlockId = string;

type BlockIdsPluginState = {
  idsByNode: Map<NodeId, BlockId>;
  decorations: DecorationSet;
};

const BLOCK_ID_NODES = new Set([
  "paragraph",
  "heading",
  "code_block",
  "blockquote",
  "bullet_list",
  "ordered_list",
  "horizontal_rule",
  "vellum_live_query",
  "vellum_result"
]);

const PRIMITIVE_NODES = new Set(["vellum_live_query", "vellum_result"]);

export const blockIdsPluginKey = new PluginKey<BlockIdsPluginState>("vellumBlockIds");

function textAttr(node: ProseMirrorNode, name: string): string | null {
  const value = node.attrs[name];
  return typeof value === "string" && value.length > 0 ? value : null;
}

function isTrackedTopLevelNode(node: ProseMirrorNode): boolean {
  return BLOCK_ID_NODES.has(node.type.name);
}

function blockIdForNode(node: ProseMirrorNode): BlockId | null {
  return textAttr(node, "id");
}

function buildPluginState(doc: ProseMirrorNode): BlockIdsPluginState {
  const idsByNode = new Map<NodeId, BlockId>();
  const decorations: Decoration[] = [];

  doc.forEach((node, offset) => {
    if (!isTrackedTopLevelNode(node)) {
      return;
    }

    const blockId = blockIdForNode(node);
    if (!blockId) {
      return;
    }

    idsByNode.set(blockId, blockId);
    decorations.push(
      Decoration.node(offset, offset + node.nodeSize, {
        "data-block-id": blockId
      })
    );
  });

  return {
    idsByNode,
    decorations: DecorationSet.create(doc, decorations)
  };
}

function transactionWithAssignedIds(state: EditorState): Transaction | null {
  const tr = state.tr;
  let changed = false;

  state.doc.forEach((node, offset) => {
    if (!isTrackedTopLevelNode(node) || PRIMITIVE_NODES.has(node.type.name) || blockIdForNode(node)) {
      return;
    }

    changed = true;
    tr.setNodeMarkup(offset, undefined, {
      ...node.attrs,
      id: crypto.randomUUID()
    });
  });

  return changed ? tr : null;
}

function dispatchAssignments(view: EditorView): void {
  const tr = transactionWithAssignedIds(view.state);
  if (tr) {
    view.dispatch(tr);
  }
}

function scheduleAssignments(view: EditorView): void {
  queueMicrotask(() => {
    if (!view.isDestroyed) {
      dispatchAssignments(view);
    }
  });
}

export function getBlockIds(state: EditorState): BlockId[] {
  const pluginState = blockIdsPluginKey.getState(state);
  return pluginState ? Array.from(pluginState.idsByNode.values()) : [];
}

export function blockIdsPlugin(): Plugin<BlockIdsPluginState> {
  return new Plugin<BlockIdsPluginState>({
    key: blockIdsPluginKey,
    state: {
      init: (_, state) => buildPluginState(state.doc),
      apply: (tr) => buildPluginState(tr.doc)
    },
    props: {
      decorations: (state) => blockIdsPluginKey.getState(state)?.decorations ?? null
    },
    view: (view) => {
      scheduleAssignments(view);

      return {
        update: (nextView) => {
          scheduleAssignments(nextView);
        }
      };
    }
  });
}
