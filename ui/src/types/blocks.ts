import { z } from "zod";

export const BlockKindSchema = z.enum([
  "Frontmatter",
  "Heading",
  "Paragraph",
  "List",
  "BlockQuote",
  "CodeBlock",
  "HtmlBlock",
  "Table",
  "FootnoteDefinition",
  "LinkRefDefinition",
  "ThematicBreak",
  "VellumLiveQuery",
  "VellumResult"
]);

export const ByteRangeSchema = z.object({
  start: z.number().int().nonnegative(),
  end: z.number().int().nonnegative()
});

export const BlockSchema = z.object({
  kind: BlockKindSchema,
  id: z.string().optional(),
  byte_range: ByteRangeSchema,
  raw_source: ByteRangeSchema
});

export const ParseErrorSchema = z.string();

export type BlockKind = z.infer<typeof BlockKindSchema>;
export type ByteRange = z.infer<typeof ByteRangeSchema>;
export type Block = z.infer<typeof BlockSchema>;
export type ParseError = z.infer<typeof ParseErrorSchema>;
