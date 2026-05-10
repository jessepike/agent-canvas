use std::{fs, ops::Range, path::PathBuf};

use uuid::Uuid;
use vellum_core::{
    SaveError,
    block::patch::{BlockEdit, BlockError, BlockPatch},
    parse::{Block, BlockKind, parse, partition::PartitionError},
    save,
};

fn corpus_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vellum-corpus/corpus")
        .join(file_name)
}

fn read_corpus(file_name: &str) -> String {
    fs::read_to_string(corpus_path(file_name)).expect("corpus fixture should be readable")
}

fn preserved_patches(blocks: &[Block]) -> Vec<BlockPatch> {
    blocks
        .iter()
        .map(|block| BlockPatch {
            block_id: Uuid::new_v4(),
            parsed_kind: block.kind,
            original_byte_range: Some(block.byte_range.clone()),
            edit: BlockEdit::PreservedBytes,
            dirty: false,
            error: None,
        })
        .collect()
}

fn inserted_patch(contents: &str) -> BlockPatch {
    BlockPatch {
        block_id: Uuid::new_v4(),
        parsed_kind: BlockKind::Paragraph,
        original_byte_range: None,
        edit: BlockEdit::EditedBytes(contents.to_owned()),
        dirty: true,
        error: None,
    }
}

fn assert_bytes_eq(actual: &str, expected: &str) {
    if actual.as_bytes() == expected.as_bytes() {
        return;
    }

    panic!(
        "saved bytes differed\nexpected:\n{}\nactual:\n{}\n",
        render_bytes(expected),
        render_bytes(actual)
    );
}

fn render_bytes(contents: &str) -> String {
    contents
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(index, byte)| format!("{index:04}: {byte:02x} {:?}\n", char::from(*byte)))
        .collect()
}

fn replace_range(source: &str, range: Range<usize>, replacement: &str) -> String {
    format!(
        "{}{}{}",
        &source[..range.start],
        replacement,
        &source[range.end..]
    )
}

fn first_block_of_kind(blocks: &[Block], kind: BlockKind) -> usize {
    blocks
        .iter()
        .position(|block| block.kind == kind)
        .expect("fixture should contain requested block kind")
}

#[test]
fn preserves_frontmatter_yaml_byte_identical() {
    let source = read_corpus("frontmatter-yaml.md");
    let blocks = parse(&source).expect("fixture should parse");
    let saved = save(&source, &preserved_patches(&blocks)).expect("save should succeed");

    assert_bytes_eq(&saved, &source);
}

#[test]
fn preserves_list_tight_byte_identical() {
    let source = read_corpus("list-tight.md");
    let blocks = parse(&source).expect("fixture should parse");
    let saved = save(&source, &preserved_patches(&blocks)).expect("save should succeed");

    assert_bytes_eq(&saved, &source);
}

#[test]
fn preserves_table_default_alignment_byte_identical() {
    let source = read_corpus("table-default-alignment.md");
    let blocks = parse(&source).expect("fixture should parse");
    let saved = save(&source, &preserved_patches(&blocks)).expect("save should succeed");

    assert_bytes_eq(&saved, &source);
}

#[test]
fn preserves_vellum_live_query_block_byte_identical() {
    let source = read_corpus("vellum-live-query-block.md");
    let blocks = parse(&source).expect("fixture should parse");
    let saved = save(&source, &preserved_patches(&blocks)).expect("save should succeed");

    assert_bytes_eq(&saved, &source);
}

#[test]
fn edits_single_paragraph_with_edited_bytes() {
    let source = read_corpus("frontmatter-yaml.md");
    let blocks = parse(&source).expect("fixture should parse");
    let paragraph_index = first_block_of_kind(&blocks, BlockKind::Paragraph);
    let mut patches = preserved_patches(&blocks);
    let replacement = "Rewritten paragraph.\n";

    patches[paragraph_index].edit = BlockEdit::EditedBytes(replacement.to_owned());
    patches[paragraph_index].dirty = true;

    let saved = save(&source, &patches).expect("save should succeed");
    let expected = replace_range(
        &source,
        blocks[paragraph_index].byte_range.clone(),
        replacement,
    );

    assert_bytes_eq(&saved, &expected);
}

#[test]
fn inserts_new_block_between_existing_patches() {
    let source = read_corpus("frontmatter-yaml.md");
    let blocks = parse(&source).expect("fixture should parse");
    let mut patches = preserved_patches(&blocks);
    let inserted = "Inserted paragraph.\n\n";
    let insert_at = blocks[0].byte_range.end;

    patches.insert(1, inserted_patch(inserted));

    let saved = save(&source, &patches).expect("save should succeed");
    let expected = format!(
        "{}{}{}",
        &source[..insert_at],
        inserted,
        &source[insert_at..]
    );

    assert_bytes_eq(&saved, &expected);
}

#[test]
fn deletes_block_by_omitting_its_patch() {
    let source = read_corpus("paragraph-reflow-risk.md");
    let blocks = parse(&source).expect("fixture should parse");
    let delete_index = first_block_of_kind(&blocks[1..], BlockKind::Paragraph) + 1;
    let mut patches = preserved_patches(&blocks);
    let deleted_range = blocks[delete_index].byte_range.clone();

    patches.remove(delete_index);

    let saved = save(&source, &patches).expect("save should succeed");
    let expected = replace_range(&source, deleted_range, "");

    assert_bytes_eq(&saved, &expected);
}

#[test]
fn merges_two_paragraphs_by_replacing_first_and_omitting_second() {
    let source = read_corpus("paragraph-reflow-risk.md");
    let blocks = parse(&source).expect("fixture should parse");
    let first_index = first_block_of_kind(&blocks, BlockKind::Paragraph);
    let second_index =
        first_block_of_kind(&blocks[first_index + 1..], BlockKind::Paragraph) + first_index + 1;
    let mut patches = preserved_patches(&blocks);
    let merged = "Merged paragraph from two source blocks.\n";
    let merged_range = blocks[first_index].byte_range.start..blocks[second_index].byte_range.end;

    patches[first_index].original_byte_range = None;
    patches[first_index].edit = BlockEdit::EditedBytes(merged.to_owned());
    patches[first_index].dirty = true;
    patches.remove(second_index);

    let saved = save(&source, &patches).expect("save should succeed");
    let expected = replace_range(&source, merged_range, merged);

    assert_bytes_eq(&saved, &expected);
}

#[test]
fn rejects_preserved_patch_without_original_bytes() {
    let source = read_corpus("list-tight.md");
    let mut patches = preserved_patches(&parse(&source).expect("fixture should parse"));
    let block_id = patches[0].block_id;

    patches[0].original_byte_range = None;

    let error = save(&source, &patches).expect_err("save should reject missing range");
    assert!(matches!(error, SaveError::MissingOriginalBytes { block_id: id } if id == block_id));
}

#[test]
fn rejects_patch_with_validation_error() {
    let source = read_corpus("list-tight.md");
    let mut patches = preserved_patches(&parse(&source).expect("fixture should parse"));
    let other_id = Uuid::new_v4();

    patches[0].error = Some(BlockError::Overlapping(other_id));

    let error = save(&source, &patches).expect_err("save should reject validation error");
    assert!(matches!(
        error,
        SaveError::PatchValidation(BlockError::Overlapping(id)) if id == other_id
    ));
}

#[test]
fn rejects_overlapping_input_ranges() {
    let source = read_corpus("frontmatter-yaml.md");
    let blocks = parse(&source).expect("fixture should parse");
    let mut patches = preserved_patches(&blocks);

    patches[1].original_byte_range = Some(blocks[0].byte_range.end - 1..blocks[1].byte_range.end);

    let error = save(&source, &patches).expect_err("save should reject overlap");
    assert!(matches!(
        error,
        SaveError::PartitionInvariant(PartitionError::Overlap { index: 1, .. })
    ));
}

#[test]
fn serialize_from_tree_is_unimplemented_for_all_block_kinds() {
    let source = read_corpus("list-tight.md");
    let mut patches = preserved_patches(&parse(&source).expect("fixture should parse"));

    patches[0].edit = BlockEdit::SerializeFromTree;
    patches[0].dirty = true;

    let error = save(&source, &patches).expect_err("save should reject serializer path");
    assert!(matches!(
        error,
        SaveError::SerializerUnimplemented {
            kind: BlockKind::List
        }
    ));
}
