import type { Node as ProseMirrorNode } from "prosemirror-model";
import type { NodeView } from "prosemirror-view";

function textAttr(node: ProseMirrorNode, name: string, fallback: string): string {
  const value = node.attrs[name];
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

function lineCount(raw: unknown): number {
  if (typeof raw !== "string" || raw.length === 0) {
    return 0;
  }

  return raw.split(/\r\n|\r|\n/).length;
}

export function liveQueryNodeView(node: ProseMirrorNode): NodeView {
  const dom = document.createElement("section");
  dom.className = "pm-primitive pm-live-query";
  dom.contentEditable = "false";

  const label = document.createElement("span");
  label.className = "pm-primitive-label";
  label.textContent = textAttr(node, "tool", "unknown tool");

  const badge = document.createElement("span");
  badge.className = "pm-badge";
  badge.textContent = "recipe";

  dom.append(label, badge);

  return { dom };
}

export function resultNodeView(): NodeView {
  const dom = document.createElement("section");
  dom.className = "pm-primitive pm-result";
  dom.contentEditable = "false";

  const label = document.createElement("span");
  label.className = "pm-primitive-label";
  label.textContent = "vellum:result";

  const badge = document.createElement("span");
  badge.className = "pm-badge pm-badge-pending";
  badge.textContent = "evidence-state pending";

  dom.append(label, badge);

  return { dom };
}

export function frontmatterNodeView(node: ProseMirrorNode): NodeView {
  const dom = document.createElement("section");
  dom.className = "pm-frontmatter";
  dom.contentEditable = "false";

  const kind = textAttr(node, "kind", "yaml");
  const lines = lineCount(node.attrs.raw);
  dom.textContent = `${kind} frontmatter - ${lines} lines`;

  return { dom };
}
