import type { Mark, Node as ProseMirrorNode, Schema } from "prosemirror-model";
import type { Inline } from "../types/blocks";

function textNode(schema: Schema, text: string, marks: readonly Mark[] = []): ProseMirrorNode | null {
  return text.length > 0 ? schema.text(text, marks) : null;
}

function appendText(
  schema: Schema,
  nodes: ProseMirrorNode[],
  text: string,
  marks: readonly Mark[]
): void {
  const node = textNode(schema, text, marks);
  if (node) {
    nodes.push(node);
  }
}

export function inlinesToPmInlines(
  schema: Schema,
  inlines: Inline[],
  marks: readonly Mark[] = []
): ProseMirrorNode[] {
  const nodes: ProseMirrorNode[] = [];

  for (const inline of inlines) {
    if (typeof inline === "string") {
      if (inline === "HardBreak") {
        nodes.push(schema.nodes.hard_break.create());
      } else if (inline === "SoftBreak") {
        appendText(schema, nodes, "\n", marks);
      }
    } else if ("Text" in inline) {
      appendText(schema, nodes, inline.Text, marks);
    } else if ("Strong" in inline) {
      nodes.push(...inlinesToPmInlines(schema, inline.Strong, [...marks, schema.marks.strong.create()]));
    } else if ("Emphasis" in inline) {
      nodes.push(...inlinesToPmInlines(schema, inline.Emphasis, [...marks, schema.marks.em.create()]));
    } else if ("Code" in inline) {
      appendText(schema, nodes, inline.Code, [...marks, schema.marks.code.create()]);
    } else if ("Link" in inline) {
      const linkMark = schema.marks.link.create({
        href: inline.Link.href,
        title: inline.Link.title
      });
      nodes.push(...inlinesToPmInlines(schema, inline.Link.body, [...marks, linkMark]));
    } else if ("Image" in inline) {
      appendText(schema, nodes, inline.Image.alt || inline.Image.src, marks);
    } else if ("Html" in inline) {
      appendText(schema, nodes, inline.Html, marks);
    }
  }

  return nodes;
}

export function pmTextNode(schema: Schema, text: string): ProseMirrorNode | null {
  return textNode(schema, text);
}

export function plainTextFromInlines(inlines: Inline[]): string {
  return inlines.map(inlinePlainText).join("");
}

function inlinePlainText(inline: Inline): string {
  if (typeof inline === "string") {
    return inline === "HardBreak" || inline === "SoftBreak" ? "\n" : "";
  }
  if ("Text" in inline) {
    return inline.Text;
  }
  if ("Strong" in inline) {
    return plainTextFromInlines(inline.Strong);
  }
  if ("Emphasis" in inline) {
    return plainTextFromInlines(inline.Emphasis);
  }
  if ("Code" in inline) {
    return inline.Code;
  }
  if ("Link" in inline) {
    return plainTextFromInlines(inline.Link.body);
  }
  if ("Image" in inline) {
    return inline.Image.alt;
  }
  return inline.Html;
}
