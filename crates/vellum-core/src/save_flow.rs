use thiserror::Error;

use crate::{
    block::patch::{BlockEdit, BlockError, BlockId, BlockPatch},
    parse::{Block, BlockKind, ByteRange, partition::PartitionError},
};

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("input partition invariant failed: {0}")]
    PartitionInvariant(#[from] PartitionError),
    #[error("patch validation failed: {0:?}")]
    PatchValidation(BlockError),
    #[error("missing original bytes for block {block_id}")]
    MissingOriginalBytes { block_id: BlockId },
    #[error("serializer is not implemented for {kind:?}")]
    SerializerUnimplemented { kind: BlockKind },
}

pub fn save(source: &str, patches: &[BlockPatch]) -> Result<String, SaveError> {
    validate_patches(source, patches)?;

    let mut output = String::with_capacity(source.len());
    for patch in patches {
        match &patch.edit {
            BlockEdit::PreservedBytes => {
                let range =
                    patch
                        .original_byte_range
                        .clone()
                        .ok_or(SaveError::MissingOriginalBytes {
                            block_id: patch.block_id,
                        })?;
                output.push_str(source_slice(source, &range)?);
            }
            BlockEdit::EditedBytes(contents) => output.push_str(contents),
            BlockEdit::SerializeFromTree => {
                // Stub by design: v1 editor saves source-view and primitive-body edits as
                // EditedBytes. Rust gets a serializer only after the rendered-view structured
                // representation is specified.
                return Err(SaveError::SerializerUnimplemented {
                    kind: patch.parsed_kind,
                });
            }
        }
    }

    Ok(output)
}

fn validate_patches(source: &str, patches: &[BlockPatch]) -> Result<(), SaveError> {
    for patch in patches {
        if let Some(error) = &patch.error {
            return Err(SaveError::PatchValidation(error.clone()));
        }

        if matches!(patch.edit, BlockEdit::PreservedBytes) && patch.original_byte_range.is_none() {
            return Err(SaveError::MissingOriginalBytes {
                block_id: patch.block_id,
            });
        }
    }

    verify_patch_ranges(source, patches)
}

fn verify_patch_ranges(source: &str, patches: &[BlockPatch]) -> Result<(), SaveError> {
    let blocks = patches
        .iter()
        .filter_map(|patch| {
            patch
                .original_byte_range
                .clone()
                .map(|range| block_for_partition(patch.parsed_kind, range.clone()))
        })
        .collect::<Vec<_>>();

    verify_ranges_are_ordered_and_non_overlapping(source, &blocks).map_err(Into::into)
}

fn block_for_partition(kind: BlockKind, byte_range: ByteRange) -> Block {
    Block {
        kind,
        raw_source: byte_range.clone(),
        byte_range,
    }
}

fn verify_ranges_are_ordered_and_non_overlapping(
    source: &str,
    blocks: &[Block],
) -> Result<(), PartitionError> {
    let mut previous_end = 0;

    for (index, block) in blocks.iter().enumerate() {
        let start = block.byte_range.start;
        let end = block.byte_range.end;

        if start > end || end > source.len() {
            return Err(PartitionError::InvalidRange { index, start, end });
        }

        if source.get(start..end).is_none() {
            return Err(PartitionError::InvalidRange { index, start, end });
        }

        if start < previous_end {
            return Err(PartitionError::Overlap {
                index,
                previous_end,
                actual_start: start,
            });
        }

        previous_end = end;
    }

    Ok(())
}

fn source_slice<'source>(
    source: &'source str,
    range: &ByteRange,
) -> Result<&'source str, SaveError> {
    source
        .get(range.start..range.end)
        .ok_or(PartitionError::InvalidRange {
            index: 0,
            start: range.start,
            end: range.end,
        })
        .map_err(Into::into)
}
