use std::ops::Range;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod partition;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub kind: BlockKind,
    pub byte_range: Range<usize>,
    pub raw_source: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parser is not implemented yet")]
    Unimplemented,
}

/// Parses Markdown into an ordered byte-span partition of top-level blocks.
///
/// The partition contract is strict: emitted blocks must cover `0..source.len()`
/// exactly, in source order, with no gaps, no overlaps, and no nested
/// top-level spans. Concatenating every `byte_range` in order must reproduce
/// the original source bytes byte-for-byte. `raw_source` preserves the bytes
/// Vellum can emit verbatim for untouched blocks.
pub fn parse(source: &str) -> Result<Vec<Block>, ParseError> {
    let _ = source;
    todo!("Gate 30A parser implementation is intentionally deferred")
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert!(true);
    }
}
