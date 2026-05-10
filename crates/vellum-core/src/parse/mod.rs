use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod partition;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct Block {
    pub kind: BlockKind,
    pub byte_range: ByteRange,
    pub raw_source: ByteRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum BlockKind {
    Frontmatter,
    Heading,
    Paragraph,
    List,
    BlockQuote,
    CodeBlock,
    HtmlBlock,
    Table,
    FootnoteDefinition,
    LinkRefDefinition,
    ThematicBreak,
    VellumLiveQuery,
    VellumResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn into_range(self) -> Range<usize> {
        self.start..self.end
    }
}

impl From<Range<usize>> for ByteRange {
    fn from(value: Range<usize>) -> Self {
        Self::new(value.start, value.end)
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parser is not implemented yet")]
    Unimplemented,
    #[error("partition invariant failed: {0}")]
    PartitionInvariant(#[from] partition::PartitionError),
}

/// Parses Markdown into an ordered byte-span partition of top-level blocks.
///
/// The partition contract is strict: emitted blocks must cover `0..source.len()`
/// exactly, in source order, with no gaps, no overlaps, and no nested
/// top-level spans. Concatenating every `byte_range` in order must reproduce
/// the original source bytes byte-for-byte. `raw_source` preserves the bytes
/// Vellum can emit verbatim for untouched blocks.
pub fn parse(source: &str) -> Result<Vec<Block>, ParseError> {
    if source.is_empty() {
        return Ok(Vec::new());
    }

    if source.trim().is_empty() {
        let block = Block {
            kind: BlockKind::Paragraph,
            byte_range: ByteRange::new(0, source.len()),
            raw_source: ByteRange::new(0, source.len()),
        };
        return Ok(vec![block]);
    }

    let mut blocks = Vec::new();
    let body_start = frontmatter_range(source).map_or(0, |range| {
        blocks.push(block(BlockKind::Frontmatter, range.clone()));
        range.end
    });

    blocks.extend(link_ref_definition_blocks(source, body_start));
    blocks.extend(markdown_blocks(source, body_start));

    blocks.sort_by_key(|block| (block.byte_range.start, block.byte_range.end));
    blocks.dedup_by(|right, left| left.byte_range == right.byte_range && left.kind == right.kind);

    stitch_partition(source, &mut blocks);
    partition::verify_partition(source, &blocks)?;

    Ok(blocks)
}

fn block(kind: BlockKind, byte_range: Range<usize>) -> Block {
    let byte_range = ByteRange::from(byte_range);
    Block {
        kind,
        raw_source: byte_range.clone(),
        byte_range,
    }
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_HEADING_ATTRIBUTES
}

fn markdown_blocks(source: &str, offset: usize) -> Vec<Block> {
    let parser = Parser::new_ext(&source[offset..], parser_options()).into_offset_iter();
    let mut blocks = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<(BlockKind, usize)> = None;

    for (event, range) in parser {
        let range = (range.start + offset)..(range.end + offset);

        match event {
            Event::Start(tag) => {
                if depth == 0
                    && let Some(kind) = block_kind_for_start(&tag)
                {
                    current = Some((kind, range.start));
                }
                depth += 1;
            }
            Event::End(_) => {
                if depth == 1
                    && let Some((kind, start)) = current.take()
                {
                    blocks.push(block(kind, start..range.end));
                }
                depth = depth.saturating_sub(1);
            }
            Event::Rule if depth == 0 => {
                blocks.push(block(BlockKind::ThematicBreak, range));
            }
            _ => {}
        }
    }

    blocks
}

fn block_kind_for_start(tag: &Tag<'_>) -> Option<BlockKind> {
    match tag {
        Tag::Paragraph => Some(BlockKind::Paragraph),
        Tag::Heading { .. } => Some(BlockKind::Heading),
        Tag::List(_) => Some(BlockKind::List),
        Tag::BlockQuote(_) => Some(BlockKind::BlockQuote),
        Tag::CodeBlock(kind) => Some(code_block_kind(kind)),
        Tag::HtmlBlock => Some(BlockKind::HtmlBlock),
        Tag::Table(_) => Some(BlockKind::Table),
        Tag::FootnoteDefinition(_) => Some(BlockKind::FootnoteDefinition),
        _ => None,
    }
}

fn code_block_kind(kind: &CodeBlockKind<'_>) -> BlockKind {
    match kind {
        CodeBlockKind::Fenced(info) if info.as_ref() == "vellum:live-query" => {
            BlockKind::VellumLiveQuery
        }
        CodeBlockKind::Fenced(info) if info.as_ref() == "vellum:result" => BlockKind::VellumResult,
        _ => BlockKind::CodeBlock,
    }
}

fn stitch_partition(source: &str, blocks: &mut [Block]) {
    if blocks.is_empty() {
        return;
    }

    blocks[0].byte_range.start = 0;
    blocks[0].raw_source.start = 0;

    for index in 0..blocks.len() {
        let end = blocks
            .get(index + 1)
            .map_or(source.len(), |next| next.byte_range.start);
        blocks[index].byte_range.end = end;
        blocks[index].raw_source = blocks[index].byte_range.clone();
    }
}

fn frontmatter_range(source: &str) -> Option<Range<usize>> {
    delimited_frontmatter_range(source, "---")
        .or_else(|| delimited_frontmatter_range(source, "+++"))
        .or_else(|| json_frontmatter_range(source))
}

fn delimited_frontmatter_range(source: &str, delimiter: &str) -> Option<Range<usize>> {
    let first = next_line(source, 0)?;
    if line_without_ending(source, first.clone()) != delimiter {
        return None;
    }

    let mut offset = first.end;
    while offset < source.len() {
        let line = next_line(source, offset)?;
        if line_without_ending(source, line.clone()) == delimiter {
            return Some(0..line.end);
        }
        offset = line.end;
    }

    None
}

fn json_frontmatter_range(source: &str) -> Option<Range<usize>> {
    let first = next_line(source, 0)?;
    if line_without_ending(source, first.clone()) != "{" {
        return None;
    }

    let mut offset = first.end;
    while offset < source.len() {
        let line = next_line(source, offset)?;
        if line_without_ending(source, line.clone()) == "}" {
            return Some(0..line.end);
        }
        offset = line.end;
    }

    None
}

fn link_ref_definition_blocks(source: &str, offset: usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut cursor = offset;

    while cursor < source.len() {
        let Some(line) = next_line(source, cursor) else {
            break;
        };

        if is_link_ref_definition_line(line_without_ending(source, line.clone())) {
            blocks.push(block(BlockKind::LinkRefDefinition, line.clone()));
        }

        cursor = line.end;
    }

    blocks
}

fn is_link_ref_definition_line(line: &str) -> bool {
    if !line.starts_with('[') {
        return false;
    }

    let Some(label_end) = line.find("]:") else {
        return false;
    };

    label_end > 1 && line[label_end + 2..].starts_with(|ch: char| ch.is_whitespace())
}

fn next_line(source: &str, offset: usize) -> Option<Range<usize>> {
    if offset >= source.len() {
        return None;
    }

    let rest = &source[offset..];
    let end = rest
        .find('\n')
        .map_or(source.len(), |newline| offset + newline + 1);
    Some(offset..end)
}

fn line_without_ending(source: &str, range: Range<usize>) -> &str {
    let mut end = range.end;
    if end > range.start && source.as_bytes()[end - 1] == b'\n' {
        end -= 1;
    }
    if end > range.start && source.as_bytes()[end - 1] == b'\r' {
        end -= 1;
    }
    &source[range.start..end]
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert!(true);
    }
}
