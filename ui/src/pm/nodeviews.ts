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

function applyBlockId(dom: HTMLElement, node: ProseMirrorNode): void {
  const blockId = textAttr(node, "id", "");
  if (blockId) {
    dom.dataset.blockId = blockId;
  }
}

function appendYamlError(dom: HTMLElement, node: ProseMirrorNode): boolean {
  const error = textAttr(node, "yaml_error", "");
  if (!error) {
    return false;
  }

  const badge = document.createElement("span");
  badge.className = "pm-badge pm-badge-error";
  badge.textContent = "invalid YAML";

  const message = document.createElement("span");
  message.className = "pm-primitive-error";
  message.textContent = error;

  const raw = document.createElement("pre");
  raw.className = "pm-primitive-raw";
  raw.textContent = textAttr(node, "raw_yaml", "");

  dom.append(badge, message, raw);
  return true;
}

export function liveQueryNodeView(node: ProseMirrorNode): NodeView {
  const dom = document.createElement("section");
  dom.className = "pm-primitive pm-live-query";
  dom.contentEditable = "false";
  applyBlockId(dom, node);

  const label = document.createElement("span");
  label.className = "pm-primitive-label";
  label.textContent = textAttr(node, "tool", "unknown tool");

  if (!appendYamlError(dom, node)) {
    const badge = document.createElement("span");
    badge.className = "pm-badge";
    badge.textContent = "recipe";

    dom.append(label, badge);
  } else {
    dom.prepend(label);
  }

  return { dom };
}

export function resultNodeView(node: ProseMirrorNode): NodeView {
  const dom = document.createElement("section");
  dom.className = "pm-primitive pm-result";
  dom.contentEditable = "false";
  applyBlockId(dom, node);

  const label = document.createElement("span");
  label.className = "pm-primitive-label";
  label.textContent = "vellum:result";

  if (!appendYamlError(dom, node)) {
    const badge = document.createElement("span");
    badge.className = "pm-badge pm-badge-pending";
    badge.textContent = "evidence-state pending";

    dom.append(label, badge);
  } else {
    dom.prepend(label);
  }

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
