use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod partition;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct Block {
    pub kind: BlockKind,
    pub byte_range: ByteRange,
    pub payload: BlockPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum BlockPayload {
    Frontmatter {
        kind: FrontmatterKind,
        raw: String,
    },
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    CodeBlock {
        language: Option<String>,
        content: String,
    },
    BlockQuote {
        children: Vec<Block>,
    },
    List {
        ordered: bool,
        #[ts(type = "number | null")]
        start: Option<u64>,
        tight: bool,
        items: Vec<ListItem>,
    },
    ThematicBreak,
    VellumLiveQuery {
        yaml: String,
    },
    VellumResult {
        yaml: String,
    },
    HtmlBlock {
        html: String,
    },
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    FootnoteDefinition {
        label: String,
        children: Vec<Block>,
    },
    LinkRefDefinition {
        label: String,
        dest: String,
        title: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum FrontmatterKind {
    Yaml,
    Toml,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Code(String),
    Link {
        href: String,
        title: Option<String>,
        body: Vec<Inline>,
    },
    Image {
        src: String,
        title: Option<String>,
        alt: String,
    },
    HardBreak,
    SoftBreak,
    Html(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct ListItem {
    pub children: Vec<Block>,
    pub checkbox: Option<bool>,
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
/// the original source bytes byte-for-byte. Payloads are derived views and do
/// not participate in format-preserving saves.
pub fn parse(source: &str) -> Result<Vec<Block>, ParseError> {
    if source.is_empty() {
        return Ok(Vec::new());
    }

    if source.trim().is_empty() {
        let block = Block {
            kind: BlockKind::Paragraph,
            byte_range: ByteRange::new(0, source.len()),
            payload: BlockPayload::Paragraph {
                inlines: inline_payload(source),
            },
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
    populate_payloads(source, &mut blocks);
    partition::verify_partition(source, &blocks)?;

    Ok(blocks)
}

fn block(kind: BlockKind, byte_range: Range<usize>) -> Block {
    Block {
        kind,
        byte_range: ByteRange::from(byte_range),
        payload: BlockPayload::ThematicBreak,
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

    for index in 0..blocks.len() {
        let end = blocks
            .get(index + 1)
            .map_or(source.len(), |next| next.byte_range.start);
        blocks[index].byte_range.end = end;
    }
}

fn populate_payloads(source: &str, blocks: &mut [Block]) {
    for block in blocks {
        block.payload = payload_for_block(source, block.kind, block.byte_range.clone());
    }
}

fn payload_for_block(source: &str, kind: BlockKind, byte_range: ByteRange) -> BlockPayload {
    let raw = source
        .get(byte_range.start..byte_range.end)
        .unwrap_or_default();

    match kind {
        BlockKind::Frontmatter => BlockPayload::Frontmatter {
            kind: frontmatter_kind(raw),
            raw: raw.to_owned(),
        },
        BlockKind::Heading => heading_payload(raw),
        BlockKind::Paragraph => BlockPayload::Paragraph {
            inlines: inline_payload(raw),
        },
        BlockKind::List => list_payload(raw),
        BlockKind::BlockQuote => BlockPayload::BlockQuote {
            children: parse_nested_blocks(&strip_blockquote_markers(raw)),
        },
        BlockKind::CodeBlock => code_block_payload(raw),
        BlockKind::HtmlBlock => BlockPayload::HtmlBlock {
            html: html_payload(raw),
        },
        BlockKind::Table => table_payload(raw),
        BlockKind::FootnoteDefinition => footnote_payload(raw),
        BlockKind::LinkRefDefinition => link_ref_payload(raw),
        BlockKind::ThematicBreak => BlockPayload::ThematicBreak,
        BlockKind::VellumLiveQuery => BlockPayload::VellumLiveQuery {
            yaml: fenced_body(raw),
        },
        BlockKind::VellumResult => BlockPayload::VellumResult {
            yaml: fenced_body(raw),
        },
    }
}

fn parse_nested_blocks(source: &str) -> Vec<Block> {
    parse(source).unwrap_or_default()
}

fn frontmatter_kind(raw: &str) -> FrontmatterKind {
    let first = raw.lines().next().unwrap_or_default();
    match first {
        "+++" => FrontmatterKind::Toml,
        "{" => FrontmatterKind::Json,
        _ => FrontmatterKind::Yaml,
    }
}

fn heading_payload(raw: &str) -> BlockPayload {
    let mut inlines = Vec::new();
    let mut level = 1;

    let mut depth = 0usize;
    let mut in_heading = false;
    let mut collector = InlineCollector::default();

    for (event, _) in Parser::new_ext(raw, parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading {
                level: heading_level,
                ..
            }) if depth == 0 => {
                level = heading_level_u8(heading_level);
                in_heading = true;
                depth += 1;
            }
            Event::Start(tag) if in_heading => {
                collector.start(tag);
                depth += 1;
            }
            Event::End(TagEnd::Heading(_)) if depth == 1 && in_heading => {
                inlines = collector.finish();
                break;
            }
            Event::End(end) if in_heading => {
                collector.end(end);
                depth = depth.saturating_sub(1);
            }
            event if in_heading => collector.event(event),
            _ => {}
        }
    }

    BlockPayload::Heading { level, inlines }
}

fn inline_payload(raw: &str) -> Vec<Inline> {
    let mut depth = 0usize;
    let mut in_paragraph = false;
    let mut collector = InlineCollector::default();

    for (event, _) in Parser::new_ext(raw, parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Paragraph) if depth == 0 => {
                in_paragraph = true;
                depth += 1;
            }
            Event::Start(tag) if in_paragraph => {
                collector.start(tag);
                depth += 1;
            }
            Event::End(TagEnd::Paragraph) if depth == 1 && in_paragraph => break,
            Event::End(end) if in_paragraph => {
                collector.end(end);
                depth = depth.saturating_sub(1);
            }
            event if in_paragraph => collector.event(event),
            _ => {}
        }
    }

    collector.finish()
}

fn code_block_payload(raw: &str) -> BlockPayload {
    let mut language = None;
    let mut content = String::new();
    let mut in_code = false;

    for (event, _) in Parser::new_ext(raw, parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                language = code_block_language(&kind);
                in_code = true;
            }
            Event::Text(text) if in_code => content.push_str(text.as_ref()),
            Event::End(TagEnd::CodeBlock) if in_code => break,
            _ => {}
        }
    }

    BlockPayload::CodeBlock { language, content }
}

fn code_block_language(kind: &CodeBlockKind<'_>) -> Option<String> {
    match kind {
        CodeBlockKind::Fenced(info) => info
            .split_whitespace()
            .next()
            .filter(|language| !language.is_empty())
            .map(ToOwned::to_owned),
        CodeBlockKind::Indented => None,
    }
}

fn list_payload(raw: &str) -> BlockPayload {
    let mut ordered = false;
    let mut start = None;
    let mut item_ranges = Vec::new();
    let mut checkboxes = Vec::new();
    let mut depth = 0usize;
    let mut item_depth = None;
    let mut current_checkbox = None;

    for (event, range) in Parser::new_ext(raw, parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::List(list_start)) if depth == 0 => {
                ordered = list_start.is_some();
                start = list_start;
                depth += 1;
            }
            Event::Start(Tag::Item) if depth == 1 => {
                item_depth = Some(depth);
                current_checkbox = None;
                item_ranges.push(range);
                depth += 1;
            }
            Event::Start(_) => depth += 1,
            Event::TaskListMarker(checked) if item_depth.is_some() => {
                current_checkbox = Some(checked);
            }
            Event::End(TagEnd::Item) if item_depth == Some(depth.saturating_sub(1)) => {
                checkboxes.push(current_checkbox);
                item_depth = None;
                depth = depth.saturating_sub(1);
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    let tight = !raw.contains("\n\n") && !raw.contains("\r\n\r\n");
    let items = item_ranges
        .into_iter()
        .enumerate()
        .map(|(index, range)| ListItem {
            children: parse_nested_blocks(&strip_list_marker(&raw[range])),
            checkbox: checkboxes.get(index).copied().flatten(),
        })
        .collect();

    BlockPayload::List {
        ordered,
        start,
        tight,
        items,
    }
}

fn strip_list_marker(raw: &str) -> String {
    let mut output = String::new();
    let marker_indent = raw
        .lines()
        .next()
        .map(|line| line.len() - line.trim_start().len())
        .unwrap_or(0);
    let continuation_indent = marker_indent + 2;

    for (index, line_with_ending) in raw.split_inclusive('\n').enumerate() {
        let has_newline = line_with_ending.ends_with('\n');
        let line = line_with_ending
            .trim_end_matches('\n')
            .trim_end_matches('\r');
        let ending = if has_newline {
            if line_with_ending.ends_with("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
        } else {
            ""
        };

        if index == 0 {
            output.push_str(strip_first_list_line(line));
        } else {
            output.push_str(strip_indent(line, continuation_indent));
        }
        output.push_str(ending);
    }

    output
}

fn strip_first_list_line(line: &str) -> &str {
    let trimmed = line.trim_start();
    let Some(after_marker) =
        strip_unordered_marker(trimmed).or_else(|| strip_ordered_marker(trimmed))
    else {
        return trimmed;
    };

    strip_task_marker(after_marker.trim_start())
}

fn strip_unordered_marker(line: &str) -> Option<&str> {
    let mut chars = line.char_indices();
    let (_, marker) = chars.next()?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }
    let next = chars.next().map_or(line.len(), |(index, _)| index);
    Some(&line[next..])
}

fn strip_ordered_marker(line: &str) -> Option<&str> {
    let delimiter = line.find(['.', ')'])?;
    if delimiter == 0 || !line[..delimiter].chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some(&line[delimiter + 1..])
}

fn strip_task_marker(line: &str) -> &str {
    line.strip_prefix("[ ] ")
        .or_else(|| line.strip_prefix("[x] "))
        .or_else(|| line.strip_prefix("[X] "))
        .unwrap_or(line)
}

fn strip_indent(line: &str, columns: usize) -> &str {
    let mut byte_index = 0;
    let mut remaining = columns;
    for (index, ch) in line.char_indices() {
        if remaining == 0 || ch != ' ' {
            break;
        }
        byte_index = index + ch.len_utf8();
        remaining -= 1;
    }
    &line[byte_index..]
}

fn strip_blockquote_markers(raw: &str) -> String {
    let mut output = String::new();
    for line_with_ending in raw.split_inclusive('\n') {
        let has_newline = line_with_ending.ends_with('\n');
        let line = line_with_ending
            .trim_end_matches('\n')
            .trim_end_matches('\r');
        let ending = if has_newline {
            if line_with_ending.ends_with("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
        } else {
            ""
        };

        let stripped = line
            .trim_start()
            .strip_prefix('>')
            .map(|rest| rest.strip_prefix(' ').unwrap_or(rest))
            .unwrap_or(line);
        output.push_str(stripped);
        output.push_str(ending);
    }
    output
}

fn html_payload(raw: &str) -> String {
    let mut html = String::new();
    for (event, _) in Parser::new_ext(raw, parser_options()).into_offset_iter() {
        if let Event::Html(value) = event {
            html.push_str(value.as_ref());
        }
    }
    if html.is_empty() {
        raw.to_owned()
    } else {
        html
    }
}

fn table_payload(raw: &str) -> BlockPayload {
    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut current_row: Option<Vec<Vec<Inline>>> = None;
    let mut current_cell: Option<InlineCollector> = None;
    let mut in_header = false;
    let mut headers = Vec::new();

    for (event, _) in Parser::new_ext(raw, parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::TableHead) => in_header = true,
            Event::End(TagEnd::TableHead) => in_header = false,
            Event::Start(Tag::TableRow) => current_row = Some(Vec::new()),
            Event::End(TagEnd::TableRow) => {
                if let Some(row) = current_row.take() {
                    if in_header && headers.is_empty() {
                        headers = row;
                    } else {
                        rows.push(row);
                    }
                }
            }
            Event::Start(Tag::TableCell) => current_cell = Some(InlineCollector::default()),
            Event::End(TagEnd::TableCell) => {
                if let (Some(row), Some(cell)) = (&mut current_row, current_cell.take()) {
                    row.push(cell.finish());
                }
            }
            Event::Start(tag) => {
                if let Some(cell) = &mut current_cell {
                    cell.start(tag);
                }
            }
            Event::End(end) => {
                if let Some(cell) = &mut current_cell {
                    cell.end(end);
                }
            }
            event => {
                if let Some(cell) = &mut current_cell {
                    cell.event(event);
                }
            }
        }
    }

    BlockPayload::Table { headers, rows }
}

fn footnote_payload(raw: &str) -> BlockPayload {
    let mut label = String::new();
    for (event, _) in Parser::new_ext(raw, parser_options()).into_offset_iter() {
        if let Event::Start(Tag::FootnoteDefinition(value)) = event {
            label = value.to_string();
            break;
        }
    }

    BlockPayload::FootnoteDefinition {
        label,
        children: parse_nested_blocks(&strip_footnote_marker(raw)),
    }
}

fn strip_footnote_marker(raw: &str) -> String {
    let Some(first_line_end) = raw.find('\n') else {
        return String::new();
    };
    let first = &raw[..first_line_end];
    let Some(marker_end) = first.find("]:") else {
        return raw.to_owned();
    };
    let first_content = first[marker_end + 2..].trim_start();
    let mut output = String::new();
    output.push_str(first_content);
    output.push('\n');
    output.push_str(&raw[first_line_end + 1..]);
    output
}

fn link_ref_payload(raw: &str) -> BlockPayload {
    let line = raw.lines().next().unwrap_or_default();
    let Some(label_end) = line.find("]:") else {
        return BlockPayload::LinkRefDefinition {
            label: String::new(),
            dest: String::new(),
            title: None,
        };
    };
    let label = line[1..label_end].to_owned();
    let rest = line[label_end + 2..].trim();
    let mut parts = rest.splitn(2, char::is_whitespace);
    let dest = parts.next().unwrap_or_default().to_owned();
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_matches(['"', '\'', '(', ')']).to_owned());

    BlockPayload::LinkRefDefinition { label, dest, title }
}

fn fenced_body(raw: &str) -> String {
    let Some(first_line_end) = raw.find('\n') else {
        return String::new();
    };
    let body_with_closing = &raw[first_line_end + 1..];
    let mut body_lines = Vec::new();

    for line in body_with_closing.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            break;
        }
        body_lines.push(line);
    }

    if body_lines.is_empty() {
        String::new()
    } else {
        let mut body = body_lines.join("\n");
        body.push('\n');
        body
    }
}

fn heading_level_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[derive(Debug, Default)]
struct InlineCollector {
    root: Vec<Inline>,
    stack: Vec<InlineFrame>,
}

impl InlineCollector {
    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Strong => self.stack.push(InlineFrame::Strong(Vec::new())),
            Tag::Emphasis => self.stack.push(InlineFrame::Emphasis(Vec::new())),
            Tag::Link {
                dest_url, title, ..
            } => self.stack.push(InlineFrame::Link {
                href: dest_url.to_string(),
                title: non_empty_title(title.as_ref()),
                body: Vec::new(),
            }),
            Tag::Image {
                dest_url, title, ..
            } => self.stack.push(InlineFrame::Image {
                src: dest_url.to_string(),
                title: non_empty_title(title.as_ref()),
                alt: String::new(),
            }),
            _ => {}
        }
    }

    fn end(&mut self, end: TagEnd) {
        let inline = match (end, self.stack.pop()) {
            (TagEnd::Strong, Some(InlineFrame::Strong(children))) => Some(Inline::Strong(children)),
            (TagEnd::Emphasis, Some(InlineFrame::Emphasis(children))) => {
                Some(Inline::Emphasis(children))
            }
            (TagEnd::Link, Some(InlineFrame::Link { href, title, body })) => {
                Some(Inline::Link { href, title, body })
            }
            (TagEnd::Image, Some(InlineFrame::Image { src, title, alt })) => {
                Some(Inline::Image { src, title, alt })
            }
            (_, frame) => {
                if let Some(frame) = frame {
                    self.stack.push(frame);
                }
                None
            }
        };

        if let Some(inline) = inline {
            self.push(inline);
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Text(text) => self.push_text(text.as_ref()),
            Event::Code(code) => self.push(Inline::Code(code.to_string())),
            Event::InlineHtml(html) | Event::Html(html) => {
                self.push(Inline::Html(html.to_string()))
            }
            Event::HardBreak => self.push(Inline::HardBreak),
            Event::SoftBreak => self.push(Inline::SoftBreak),
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<Inline> {
        while let Some(frame) = self.stack.pop() {
            self.root.push(frame.into_inline());
        }
        self.root
    }

    fn push_text(&mut self, text: &str) {
        if let Some(InlineFrame::Image { alt, .. }) = self.stack.last_mut() {
            alt.push_str(text);
        } else {
            self.push(Inline::Text(text.to_owned()));
        }
    }

    fn push(&mut self, inline: Inline) {
        match self.stack.last_mut() {
            Some(InlineFrame::Strong(children))
            | Some(InlineFrame::Emphasis(children))
            | Some(InlineFrame::Link { body: children, .. }) => children.push(inline),
            Some(InlineFrame::Image { alt, .. }) => alt.push_str(&inline_plain_text(&inline)),
            None => self.root.push(inline),
        }
    }
}

#[derive(Debug)]
enum InlineFrame {
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Link {
        href: String,
        title: Option<String>,
        body: Vec<Inline>,
    },
    Image {
        src: String,
        title: Option<String>,
        alt: String,
    },
}

impl InlineFrame {
    fn into_inline(self) -> Inline {
        match self {
            InlineFrame::Strong(children) => Inline::Strong(children),
            InlineFrame::Emphasis(children) => Inline::Emphasis(children),
            InlineFrame::Link { href, title, body } => Inline::Link { href, title, body },
            InlineFrame::Image { src, title, alt } => Inline::Image { src, title, alt },
        }
    }
}

fn non_empty_title(title: &str) -> Option<String> {
    if title.is_empty() {
        None
    } else {
        Some(title.to_owned())
    }
}

fn inline_plain_text(inline: &Inline) -> String {
    match inline {
        Inline::Text(value) | Inline::Code(value) | Inline::Html(value) => value.clone(),
        Inline::Strong(children) | Inline::Emphasis(children) => inlines_plain_text(children),
        Inline::Link { body, .. } => inlines_plain_text(body),
        Inline::Image { alt, .. } => alt.clone(),
        Inline::HardBreak | Inline::SoftBreak => "\n".to_owned(),
    }
}

fn inlines_plain_text(inlines: &[Inline]) -> String {
    inlines.iter().map(inline_plain_text).collect()
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
    use super::*;

    #[test]
    fn extracts_heading_levels_one_through_six() {
        let source = "# H1\n\n## H2\n\n### H3\n\n#### H4\n\n##### H5\n\n###### H6\n";
        let blocks = parse(source).unwrap();
        let levels = blocks
            .iter()
            .filter_map(|block| match &block.payload {
                BlockPayload::Heading { level, .. } => Some(*level),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn extracts_paragraph_inline_payloads() {
        let blocks =
            parse("Plain **bold** *em* `code` [link](https://example.com \"Title\").").unwrap();
        let BlockPayload::Paragraph { inlines } = &blocks[0].payload else {
            panic!("expected paragraph payload");
        };

        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Strong(children) if children == &vec![Inline::Text("bold".to_owned())])));
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Emphasis(children) if children == &vec![Inline::Text("em".to_owned())])));
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Code(value) if value == "code"))
        );
        assert!(inlines.iter().any(|inline| {
            matches!(
                inline,
                Inline::Link { href, title, body }
                    if href == "https://example.com"
                        && title.as_deref() == Some("Title")
                        && body == &vec![Inline::Text("link".to_owned())]
            )
        }));
    }

    #[test]
    fn extracts_fenced_code_language_and_content() {
        let blocks = parse("```rust extra\nfn main() {}\n```\n").unwrap();
        let BlockPayload::CodeBlock { language, content } = &blocks[0].payload else {
            panic!("expected code block payload");
        };

        assert_eq!(language.as_deref(), Some("rust"));
        assert_eq!(content, "fn main() {}\n");
    }

    #[test]
    fn extracts_indented_code_without_language() {
        let blocks = parse("    let answer = 42;\n").unwrap();
        let BlockPayload::CodeBlock { language, content } = &blocks[0].payload else {
            panic!("expected code block payload");
        };

        assert_eq!(language, &None);
        assert_eq!(content, "let answer = 42;\n");
    }

    #[test]
    fn extracts_ordered_unordered_and_task_lists() {
        let unordered = parse("- [ ] todo\n- [x] done\n").unwrap();
        let BlockPayload::List {
            ordered,
            start,
            tight,
            items,
        } = &unordered[0].payload
        else {
            panic!("expected list payload");
        };
        assert!(!ordered);
        assert_eq!(start, &None);
        assert!(*tight);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].checkbox, Some(false));
        assert_eq!(items[1].checkbox, Some(true));

        let ordered_blocks = parse("3. third\n4. fourth\n").unwrap();
        let BlockPayload::List { ordered, start, .. } = &ordered_blocks[0].payload else {
            panic!("expected list payload");
        };
        assert!(*ordered);
        assert_eq!(*start, Some(3));
    }

    #[test]
    fn extracts_nested_blockquotes() {
        let blocks = parse("> outer\n> > inner\n").unwrap();
        let BlockPayload::BlockQuote { children } = &blocks[0].payload else {
            panic!("expected blockquote payload");
        };

        assert!(
            children
                .iter()
                .any(|block| matches!(block.payload, BlockPayload::BlockQuote { .. }))
        );
    }

    #[test]
    fn extracts_vellum_primitive_yaml() {
        let source = "```vellum:live-query\nversion: 1\nid: open-issues\ntool: github.list_issues\nargs:\n  state: open\n```\n";
        let blocks = parse(source).unwrap();
        let BlockPayload::VellumLiveQuery { yaml } = &blocks[0].payload else {
            panic!("expected vellum live query payload");
        };

        assert!(yaml.contains("version: 1\n"));
        assert!(yaml.contains("tool: github.list_issues\n"));
    }
}
