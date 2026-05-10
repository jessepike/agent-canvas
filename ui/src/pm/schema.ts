import { Schema } from "prosemirror-model";
import type { Node as ProseMirrorNode, NodeSpec, MarkSpec } from "prosemirror-model";
import type { Block } from "../types/blocks";

const nodes: Record<string, NodeSpec> = {
  doc: {
    content: "block+"
  },
  text: {
    group: "inline"
  },
  paragraph: {
    attrs: { id: { default: null } },
    content: "inline*",
    group: "block",
    toDOM: () => ["p", 0]
  },
  heading: {
    attrs: { id: { default: null }, level: { default: 1 } },
    content: "inline*",
    group: "block",
    defining: true,
    toDOM: (node) => [`h${node.attrs.level}`, 0]
  },
  code_block: {
    attrs: { id: { default: null }, language: { default: null } },
    content: "text*",
    group: "block",
    code: true,
    defining: true,
    marks: "",
    toDOM: (node) => ["pre", ["code", { "data-language": node.attrs.language ?? "" }, 0]]
  },
  bullet_list: {
    attrs: { id: { default: null } },
    content: "list_item+",
    group: "block",
    toDOM: () => ["ul", 0]
  },
  ordered_list: {
    attrs: { id: { default: null }, order: { default: 1 } },
    content: "list_item+",
    group: "block",
    toDOM: (node) => ["ol", { start: node.attrs.order === 1 ? null : node.attrs.order }, 0]
  },
  list_item: {
    content: "paragraph block*",
    toDOM: () => ["li", 0]
  },
  blockquote: {
    attrs: { id: { default: null } },
    content: "block+",
    group: "block",
    defining: true,
    toDOM: () => ["blockquote", 0]
  },
  horizontal_rule: {
    attrs: { id: { default: null } },
    group: "block",
    toDOM: () => ["hr"]
  },
  hard_break: {
    inline: true,
    group: "inline",
    selectable: false,
    toDOM: () => ["br"]
  },
  vellum_live_query: {
    attrs: {
      id: { default: null },
      version: { default: null },
      tool: { default: null },
      args: { default: null },
      render: { default: "json" },
      cache: { default: null },
      result_policy: { default: "pinned" }
    },
    atom: true,
    group: "block",
    selectable: true,
    toDOM: (node) => [
      "section",
      { "data-vellum-node": "live-query", "data-tool": node.attrs.tool ?? "" },
      node.attrs.tool ?? "vellum:live-query"
    ]
  },
  vellum_result: {
    attrs: {
      id: { default: null },
      content_hash: { default: null },
      result_hash: { default: null },
      frozen_at: { default: null }
    },
    atom: true,
    group: "block",
    selectable: true,
    toDOM: () => ["section", { "data-vellum-node": "result" }, "vellum:result"]
  },
  frontmatter: {
    attrs: {
      kind: { default: "yaml" },
      raw: { default: "" }
    },
    atom: true,
    group: "block",
    selectable: true,
    toDOM: (node) => [
      "section",
      { "data-vellum-node": "frontmatter", "data-kind": node.attrs.kind },
      `${node.attrs.kind} frontmatter`
    ]
  }
};

const marks: Record<string, MarkSpec> = {
  strong: {
    parseDOM: [{ tag: "strong" }, { tag: "b" }],
    toDOM: () => ["strong", 0]
  },
  em: {
    parseDOM: [{ tag: "em" }, { tag: "i" }],
    toDOM: () => ["em", 0]
  },
  code: {
    parseDOM: [{ tag: "code" }],
    toDOM: () => ["code", 0]
  },
  link: {
    attrs: {
      href: {},
      title: { default: null }
    },
    inclusive: false,
    parseDOM: [
      {
        tag: "a[href]",
        getAttrs: (dom) => {
          if (!(dom instanceof HTMLElement)) {
            return false;
          }

          return {
            href: dom.getAttribute("href"),
            title: dom.getAttribute("title")
          };
        }
      }
    ],
    toDOM: (node) => ["a", { href: node.attrs.href, title: node.attrs.title }, 0]
  }
};

export const vellumSchema = new Schema({ nodes, marks });

function placeholderText(block: Block): string {
  return `${block.kind} block (${block.byte_range.start}-${block.byte_range.end})`;
}

function blockAttrs(): Record<string, never> {
  return {};
}

function textBlock(
  type: "paragraph" | "heading" | "code_block",
  block: Block,
  includeBlockId = true
): ProseMirrorNode {
  const text = vellumSchema.text(placeholderText(block));
  const attrs = includeBlockId ? blockAttrs() : null;

  if (type === "heading") {
    return vellumSchema.nodes.heading.create({ ...attrs, level: 1 }, text);
  }

  return vellumSchema.nodes[type].create(attrs, text);
}

function blockToNode(block: Block): ProseMirrorNode {
  switch (block.kind) {
    case "Frontmatter":
      return vellumSchema.nodes.frontmatter.create({ kind: "yaml", raw: "" });
    case "Heading":
      return textBlock("heading", block);
    case "Paragraph":
      return textBlock("paragraph", block);
    case "CodeBlock":
      return textBlock("code_block", block);
    case "List": {
      const item = vellumSchema.nodes.list_item.create(null, textBlock("paragraph", block, false));
      return vellumSchema.nodes.bullet_list.create(blockAttrs(), item);
    }
    case "BlockQuote":
      return vellumSchema.nodes.blockquote.create(blockAttrs(), textBlock("paragraph", block, false));
    case "ThematicBreak":
      return vellumSchema.nodes.horizontal_rule.create(blockAttrs());
    case "VellumLiveQuery":
      return vellumSchema.nodes.vellum_live_query.create({
        ...blockAttrs(),
        tool: "unknown tool",
        render: "json"
      });
    case "VellumResult":
      return vellumSchema.nodes.vellum_result.create(blockAttrs());
    case "HtmlBlock":
    case "Table":
    case "FootnoteDefinition":
    case "LinkRefDefinition":
      return textBlock("paragraph", block);
  }
}

export function blocksToDoc(blocks: Block[]): ProseMirrorNode {
  const children = blocks.length > 0 ? blocks.map(blockToNode) : [vellumSchema.nodes.paragraph.create()];
  return vellumSchema.nodes.doc.create(null, children);
}
