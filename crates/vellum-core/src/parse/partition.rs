use thiserror::Error;

use super::Block;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PartitionError {
    #[error("block {index} starts at {actual_start}, expected {expected_start}")]
    Gap {
        index: usize,
        expected_start: usize,
        actual_start: usize,
    },
    #[error("block {index} starts at {actual_start}, before previous end {previous_end}")]
    Overlap {
        index: usize,
        previous_end: usize,
        actual_start: usize,
    },
    #[error("block {index} has invalid range {start}..{end}")]
    InvalidRange {
        index: usize,
        start: usize,
        end: usize,
    },
    #[error("partition ends at {actual_end}, expected {expected_end}")]
    Incomplete {
        expected_end: usize,
        actual_end: usize,
    },
}

pub fn verify_partition(source: &str, blocks: &[Block]) -> Result<(), PartitionError> {
    let mut expected_start = 0;

    for (index, block) in blocks.iter().enumerate() {
        let start = block.byte_range.start;
        let end = block.byte_range.end;

        if start > end || end > source.len() {
            return Err(PartitionError::InvalidRange { index, start, end });
        }

        if start > expected_start {
            return Err(PartitionError::Gap {
                index,
                expected_start,
                actual_start: start,
            });
        }

        if start < expected_start {
            return Err(PartitionError::Overlap {
                index,
                previous_end: expected_start,
                actual_start: start,
            });
        }

        expected_start = end;
    }

    if expected_start != source.len() {
        return Err(PartitionError::Incomplete {
            expected_end: source.len(),
            actual_end: expected_start,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Block, BlockKind, BlockPayload, ByteRange};

    fn block(start: usize, end: usize) -> Block {
        Block {
            kind: BlockKind::Paragraph,
            byte_range: ByteRange::new(start, end),
            payload: BlockPayload::Paragraph {
                inlines: Vec::new(),
            },
        }
    }

    #[test]
    fn accepts_valid_partition() {
        let source = "alpha\nbeta\n";
        let blocks = vec![block(0, 6), block(6, source.len())];

        assert_eq!(verify_partition(source, &blocks), Ok(()));
    }

    #[test]
    fn rejects_gap() {
        let source = "alpha\nbeta\n";
        let blocks = vec![block(0, 5), block(6, source.len())];

        assert!(matches!(
            verify_partition(source, &blocks),
            Err(PartitionError::Gap { index: 1, .. })
        ));
    }

    #[test]
    fn rejects_overlap() {
        let source = "alpha\nbeta\n";
        let blocks = vec![block(0, 7), block(6, source.len())];

        assert!(matches!(
            verify_partition(source, &blocks),
            Err(PartitionError::Overlap { index: 1, .. })
        ));
    }

    #[test]
    fn rejects_incomplete_partition() {
        let source = "alpha\nbeta\n";
        let blocks = vec![block(0, 6)];

        assert!(matches!(
            verify_partition(source, &blocks),
            Err(PartitionError::Incomplete { .. })
        ));
    }

    #[test]
    fn rejects_out_of_bounds_range() {
        let source = "alpha";
        let blocks = vec![block(0, 99)];

        assert!(matches!(
            verify_partition(source, &blocks),
            Err(PartitionError::InvalidRange { .. })
        ));
    }
}
