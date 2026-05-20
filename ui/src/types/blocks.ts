import { z } from "zod";
import type { Block as RsBlock } from "./generated/Block";
import type { BlockEdit as RsBlockEdit } from "./generated/BlockEdit";
import type { BlockError as RsBlockError } from "./generated/BlockError";
import type { BlockId as RsBlockId } from "./generated/BlockId";
import type { BlockIdentity as RsBlockIdentity } from "./generated/BlockIdentity";
import type { BaseSnapshot as RsBaseSnapshot } from "./generated/BaseSnapshot";
import type { Comment as RsComment } from "./generated/Comment";
import type { CommentAnchor as RsCommentAnchor } from "./generated/CommentAnchor";
import type { BlockKind as RsBlockKind } from "./generated/BlockKind";
import type { BlockPatch as RsBlockPatch } from "./generated/BlockPatch";
import type { BlockPayload as RsBlockPayload } from "./generated/BlockPayload";
import type { ByteRange as RsByteRange } from "./generated/ByteRange";
import type { FrontmatterKind as RsFrontmatterKind } from "./generated/FrontmatterKind";
import type { IdentityMap as RsIdentityMap } from "./generated/IdentityMap";
import type { Inline as RsInline } from "./generated/Inline";
import type { ListItem as RsListItem } from "./generated/ListItem";
import type { OpenDocument as RsOpenDocument } from "./generated/OpenDocument";
import type { WriteResult as RsWriteResult } from "./generated/WriteResult";

type TypeEquals<Left, Right> = Left extends Right ? (Right extends Left ? true : false) : false;

export const BlockId = z.string().uuid();
export type BlockId = z.infer<typeof BlockId>;
const _checkBlockId: TypeEquals<BlockId, RsBlockId> = true;

export const BlockKind = z.enum([
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
export type BlockKind = z.infer<typeof BlockKind>;
const _checkBlockKind: TypeEquals<BlockKind, RsBlockKind> = true;

export const ByteRange = z
  .object({
    start: z.number().int().nonnegative(),
    end: z.number().int().nonnegative()
  })
  .strict();
export type ByteRange = z.infer<typeof ByteRange>;
const _checkByteRange: TypeEquals<ByteRange, RsByteRange> = true;

export const FrontmatterKind = z.enum(["Yaml", "Toml", "Json"]);
export type FrontmatterKind = z.infer<typeof FrontmatterKind>;
const _checkFrontmatterKind: TypeEquals<FrontmatterKind, RsFrontmatterKind> = true;

type InlineType = RsInline;
export const Inline: z.ZodType<InlineType> = z.lazy(() =>
  z.union([
    z.object({ Text: z.string() }).strict(),
    z.object({ Strong: z.array(Inline) }).strict(),
    z.object({ Emphasis: z.array(Inline) }).strict(),
    z.object({ Code: z.string() }).strict(),
    z
      .object({
        Link: z
          .object({
            href: z.string(),
            title: z.string().nullable(),
            body: z.array(Inline)
          })
          .strict()
      })
      .strict(),
    z
      .object({
        Image: z
          .object({
            src: z.string(),
            title: z.string().nullable(),
            alt: z.string()
          })
          .strict()
      })
      .strict(),
    z.literal("HardBreak"),
    z.literal("SoftBreak"),
    z.object({ Html: z.string() }).strict()
  ])
);
export type Inline = z.infer<typeof Inline>;
const _checkInline: TypeEquals<Inline, RsInline> = true;

type ListItemType = RsListItem;
export const ListItem: z.ZodType<ListItemType> = z.lazy(() =>
  z
    .object({
      children: z.array(Block),
      checkbox: z.boolean().nullable()
    })
    .strict()
);
export type ListItem = z.infer<typeof ListItem>;
const _checkListItem: TypeEquals<ListItem, RsListItem> = true;

type BlockPayloadType = RsBlockPayload;
export const BlockPayload: z.ZodType<BlockPayloadType> = z.lazy(() =>
  z.union([
    z
      .object({
        Frontmatter: z
          .object({
            kind: FrontmatterKind,
            raw: z.string()
          })
          .strict()
      })
      .strict(),
    z.object({ Heading: z.object({ level: z.number(), inlines: z.array(Inline) }).strict() }).strict(),
    z.object({ Paragraph: z.object({ inlines: z.array(Inline) }).strict() }).strict(),
    z
      .object({
        CodeBlock: z
          .object({
            language: z.string().nullable(),
            content: z.string()
          })
          .strict()
      })
      .strict(),
    z.object({ BlockQuote: z.object({ children: z.array(Block) }).strict() }).strict(),
    z
      .object({
        List: z
          .object({
            ordered: z.boolean(),
            start: z.number().nullable(),
            tight: z.boolean(),
            items: z.array(ListItem)
          })
          .strict()
      })
      .strict(),
    z.literal("ThematicBreak"),
    z.object({ VellumLiveQuery: z.object({ yaml: z.string() }).strict() }).strict(),
    z.object({ VellumResult: z.object({ yaml: z.string() }).strict() }).strict(),
    z.object({ HtmlBlock: z.object({ html: z.string() }).strict() }).strict(),
    z
      .object({
        Table: z
          .object({
            headers: z.array(z.array(Inline)),
            rows: z.array(z.array(z.array(Inline)))
          })
          .strict()
      })
      .strict(),
    z
      .object({
        FootnoteDefinition: z
          .object({
            label: z.string(),
            children: z.array(Block)
          })
          .strict()
      })
      .strict(),
    z
      .object({
        LinkRefDefinition: z
          .object({
            label: z.string(),
            dest: z.string(),
            title: z.string().nullable()
          })
          .strict()
      })
      .strict()
  ])
);
export type BlockPayload = z.infer<typeof BlockPayload>;
const _checkBlockPayload: TypeEquals<BlockPayload, RsBlockPayload> = true;

export const Block = z
  .object({
    kind: BlockKind,
    byte_range: ByteRange,
    payload: BlockPayload
  })
  .strict();
export type Block = z.infer<typeof Block>;
const _checkBlock: TypeEquals<Block, RsBlock> = true;

export const BlockEdit = z.union([
  z.literal("PreservedBytes"),
  z.object({ EditedBytes: z.string() }).strict(),
  z.literal("SerializeFromTree")
]);
export type BlockEdit = z.infer<typeof BlockEdit>;
const _checkBlockEdit: TypeEquals<BlockEdit, RsBlockEdit> = true;

export const BlockError = z.union([
  z.object({ Overlapping: BlockId }).strict(),
  z.object({ GapBefore: BlockId }).strict(),
  z.object({ InvalidYaml: z.string() }).strict(),
  z.object({ DuplicateId: BlockId }).strict(),
  z.object({ MissingRequiredField: z.string() }).strict()
]);
export type BlockError = z.infer<typeof BlockError>;
const _checkBlockError: TypeEquals<BlockError, RsBlockError> = true;

export const BlockPatch = z
  .object({
    block_id: BlockId,
    parsed_kind: BlockKind,
    original_byte_range: ByteRange.nullable(),
    edit: BlockEdit,
    dirty: z.boolean(),
    error: BlockError.nullable()
  })
  .strict();
export type BlockPatch = z.infer<typeof BlockPatch>;
const _checkBlockPatch: TypeEquals<BlockPatch, RsBlockPatch> = true;

export const BlockIdentity = z
  .object({
    id: BlockId,
    byte_range_start: z.number().int().nonnegative(),
    kind: BlockKind
  })
  .strict();
export type BlockIdentity = z.infer<typeof BlockIdentity>;
const _checkBlockIdentity: TypeEquals<BlockIdentity, RsBlockIdentity> = true;

export const Hash32 = z.custom<RsIdentityMap["source_hash"]>(
  (value) =>
    Array.isArray(value) &&
    value.length === 32 &&
    value.every((item) => Number.isInteger(item) && item >= 0 && item <= 255),
  "expected 32-byte source hash"
);

export const BaseSnapshot = z
  .object({
    hash: Hash32,
    source: z.string()
  })
  .strict();
export type BaseSnapshot = z.infer<typeof BaseSnapshot>;
const _checkBaseSnapshot: TypeEquals<BaseSnapshot, RsBaseSnapshot> = true;

export const CommentAnchor = z
  .union([
    z
      .object({
        kind: z.literal("html_selection"),
        start_offset: z.number().int().nonnegative(),
        end_offset: z.number().int().nonnegative(),
        snapshot_text: z.string()
      })
      .strict(),
    z
      .object({
        kind: z.literal("text_selection").optional(),
        block_id: z.string().nullable(),
        start_offset: z.number().int().nonnegative(),
        end_offset: z.number().int().nonnegative()
      })
      .strict()
  ]);
export type CommentAnchor = z.infer<typeof CommentAnchor>;
const _checkCommentAnchor: TypeEquals<CommentAnchor, RsCommentAnchor> = true;

export const Comment = z
  .object({
    id: z.string().uuid(),
    author: z.string(),
    created_at: z.number().int(),
    anchor: CommentAnchor,
    body: z.string(),
    resolved: z.boolean()
  })
  .strict();
export type Comment = z.infer<typeof Comment>;
const _checkComment: TypeEquals<Comment, RsComment> = true;

export const IdentityMap = z
  .object({
    source_hash: Hash32,
    block_ids: z.array(BlockIdentity),
    base_snapshot: BaseSnapshot.nullable().optional(),
    comments: z.array(Comment).nullable().optional()
  })
  .strict();
export type IdentityMap = z.infer<typeof IdentityMap>;
const _checkIdentityMap: TypeEquals<IdentityMap, RsIdentityMap> = true;

export const OpenDocument = z
  .object({
    path: z.string(),
    source: z.string(),
    base_hash: Hash32,
    has_conflict_markers: z.boolean()
  })
  .strict();
export type OpenDocument = z.infer<typeof OpenDocument>;
const _checkOpenDocument: TypeEquals<OpenDocument, RsOpenDocument> = true;

export const WriteResult = z
  .object({
    new_hash: Hash32
  })
  .strict();
export type WriteResult = z.infer<typeof WriteResult>;
const _checkWriteResult: TypeEquals<WriteResult, RsWriteResult> = true;

export const BlockIdSchema = BlockId;
export const BlockKindSchema = BlockKind;
export const ByteRangeSchema = ByteRange;
export const BlockSchema = Block;
export const BlockPayloadSchema = BlockPayload;
export const FrontmatterKindSchema = FrontmatterKind;
export const InlineSchema = Inline;
export const ListItemSchema = ListItem;
export const BlockEditSchema = BlockEdit;
export const BlockErrorSchema = BlockError;
export const BlockPatchSchema = BlockPatch;
export const BlockIdentitySchema = BlockIdentity;
export const IdentityMapSchema = IdentityMap;
export const BaseSnapshotSchema = BaseSnapshot;
export const CommentAnchorSchema = CommentAnchor;
export const CommentSchema = Comment;
export const OpenDocumentSchema = OpenDocument;
export const WriteResultSchema = WriteResult;
export const ParseErrorSchema = z.string();
export type ParseError = z.infer<typeof ParseErrorSchema>;
