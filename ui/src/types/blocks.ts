import { z } from "zod";
import type { Block as RsBlock } from "./generated/Block";
import type { BlockEdit as RsBlockEdit } from "./generated/BlockEdit";
import type { BlockError as RsBlockError } from "./generated/BlockError";
import type { BlockId as RsBlockId } from "./generated/BlockId";
import type { BlockIdentity as RsBlockIdentity } from "./generated/BlockIdentity";
import type { BlockKind as RsBlockKind } from "./generated/BlockKind";
import type { BlockPatch as RsBlockPatch } from "./generated/BlockPatch";
import type { ByteRange as RsByteRange } from "./generated/ByteRange";
import type { IdentityMap as RsIdentityMap } from "./generated/IdentityMap";
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

export const Block = z
  .object({
    kind: BlockKind,
    byte_range: ByteRange,
    raw_source: ByteRange
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

export const IdentityMap = z
  .object({
    source_hash: Hash32,
    block_ids: z.array(BlockIdentity)
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
export const BlockEditSchema = BlockEdit;
export const BlockErrorSchema = BlockError;
export const BlockPatchSchema = BlockPatch;
export const BlockIdentitySchema = BlockIdentity;
export const IdentityMapSchema = IdentityMap;
export const OpenDocumentSchema = OpenDocument;
export const WriteResultSchema = WriteResult;
export const ParseErrorSchema = z.string();
export type ParseError = z.infer<typeof ParseErrorSchema>;
