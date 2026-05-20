import { Schema } from "prosemirror-model";
import type { Node as ProseMirrorNode, NodeSpec, MarkSpec } from "prosemirror-model";
import { inlinesToPmInlines, plainTextFromInlines, pmTextNode } from "./inlinePayload";
import { liveQueryAttrs, resultAttrs } from "./primitivePayload";
import type { Block, Inline, ListItem } from "../types/blocks";

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
    attrs: { checkbox: { default: null } },
    content: "paragraph block*",
    toDOM: (node) => ["li", { "data-checkbox": node.attrs.checkbox ?? "" }, 0]
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
      result_policy: { default: "pinned" },
      yaml_error: { default: null },
      raw_yaml: { default: "" }
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
      frozen_at: { default: null },
      for_id: { default: null },
      recipe_hash: { default: null },
      captured_at: { default: null },
      render: { default: "json" },
      data: { default: null },
      yaml_error: { default: null },
      raw_yaml: { default: "" }
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
  revision: {
    parseDOM: [{ tag: 'mark[data-revision="true"]' }],
    toDOM: () => ["mark", { "data-revision": "true" }, 0]
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

function blockAttrs(): { id: null } {
  return { id: null };
}

function blockToNode(block: Block): ProseMirrorNode {
  const payload = block.payload;

  if (payload === "ThematicBreak") {
    return vellumSchema.nodes.horizontal_rule.create(blockAttrs());
  }
  if ("Frontmatter" in payload) {
    return vellumSchema.nodes.frontmatter.create({
      kind: payload.Frontmatter.kind.toLowerCase(),
      raw: payload.Frontmatter.raw
    });
  }
  if ("Heading" in payload) {
    return vellumSchema.nodes.heading.create(
      { ...blockAttrs(), level: payload.Heading.level },
      inlinesToPmInlines(vellumSchema, payload.Heading.inlines)
    );
  }
  if ("Paragraph" in payload) {
    return vellumSchema.nodes.paragraph.create(
      blockAttrs(),
      inlinesToPmInlines(vellumSchema, payload.Paragraph.inlines)
    );
  }
  if ("CodeBlock" in payload) {
    const text = pmTextNode(vellumSchema, payload.CodeBlock.content);
    return vellumSchema.nodes.code_block.create(
      { ...blockAttrs(), language: payload.CodeBlock.language },
      text ? [text] : null
    );
  }
  if ("BlockQuote" in payload) {
    return vellumSchema.nodes.blockquote.create(blockAttrs(), blocksToNodes(payload.BlockQuote.children));
  }
  if ("List" in payload) {
    const items = payload.List.items.map(itemToListItem);
    if (payload.List.ordered) {
      return vellumSchema.nodes.ordered_list.create({ ...blockAttrs(), order: payload.List.start ?? 1 }, items);
    }
    return vellumSchema.nodes.bullet_list.create(blockAttrs(), items);
  }
  if ("VellumLiveQuery" in payload) {
    return liveQueryNode(payload.VellumLiveQuery.yaml);
  }
  if ("VellumResult" in payload) {
    return resultNode(payload.VellumResult.yaml);
  }
  if ("HtmlBlock" in payload) {
    return stubParagraph(payload.HtmlBlock.html);
  }
  if ("Table" in payload) {
    return stubParagraph(tableText(payload.Table.headers, payload.Table.rows));
  }
  if ("FootnoteDefinition" in payload) {
    return stubParagraph(`[^${payload.FootnoteDefinition.label}]: ${blocksPlainText(payload.FootnoteDefinition.children)}`);
  }
  if ("LinkRefDefinition" in payload) {
    const title = payload.LinkRefDefinition.title ? ` "${payload.LinkRefDefinition.title}"` : "";
    return stubParagraph(`[${payload.LinkRefDefinition.label}]: ${payload.LinkRefDefinition.dest}${title}`);
  }

  return stubParagraph(block.kind);
}

function itemToListItem(item: ListItem): ProseMirrorNode {
  const children = blocksToNodes(item.children);
  const content =
    children.length > 0 && children[0].type === vellumSchema.nodes.paragraph
      ? children
      : [vellumSchema.nodes.paragraph.create(), ...children];

  return vellumSchema.nodes.list_item.create({ checkbox: item.checkbox }, content);
}

function liveQueryNode(raw: string): ProseMirrorNode {
  return vellumSchema.nodes.vellum_live_query.create({ ...blockAttrs(), ...liveQueryAttrs(raw) });
}

function resultNode(raw: string): ProseMirrorNode {
  return vellumSchema.nodes.vellum_result.create({ ...blockAttrs(), ...resultAttrs(raw) });
}

function blocksToNodes(blocks: Block[]): ProseMirrorNode[] {
  return blocks.length > 0 ? blocks.map(blockToNode) : [vellumSchema.nodes.paragraph.create()];
}

function stubParagraph(text: string): ProseMirrorNode {
  const node = pmTextNode(vellumSchema, text);
  return vellumSchema.nodes.paragraph.create(blockAttrs(), node ? [node] : null);
}

function tableText(headers: Inline[][], rows: Inline[][][]): string {
  const headerText = headers.map(plainTextFromInlines).join(" | ");
  const rowText = rows.map((row) => row.map(plainTextFromInlines).join(" | ")).join("\n");
  return [headerText, rowText].filter(Boolean).join("\n");
}

function blocksPlainText(blocks: Block[]): string {
  return blocks.map(blockPlainText).filter(Boolean).join("\n");
}

function blockPlainText(block: Block): string {
  const payload = block.payload;
  if (payload === "ThematicBreak") {
    return "";
  }
  if ("Paragraph" in payload) {
    return plainTextFromInlines(payload.Paragraph.inlines);
  }
  if ("Heading" in payload) {
    return plainTextFromInlines(payload.Heading.inlines);
  }
  if ("CodeBlock" in payload) {
    return payload.CodeBlock.content;
  }
  if ("BlockQuote" in payload) {
    return blocksPlainText(payload.BlockQuote.children);
  }
  return "";
}

export function blocksToDoc(blocks: Block[]): ProseMirrorNode {
  return vellumSchema.nodes.doc.create(null, blocksToNodes(blocks));
}
