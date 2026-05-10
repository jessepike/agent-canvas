use std::ops::Range;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::parse::BlockKind;

pub type BlockId = Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPatch {
    pub block_id: BlockId,
    pub parsed_kind: BlockKind,
    pub original_byte_range: Option<Range<usize>>,
    pub edit: BlockEdit,
    pub dirty: bool,
    pub error: Option<BlockError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockEdit {
    PreservedBytes,
    EditedBytes(String),
    SerializeFromTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockError {
    Overlapping(BlockId),
    GapBefore(BlockId),
    InvalidYaml(String),
    DuplicateId(BlockId),
    MissingRequiredField(String),
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert!(true);
    }
}
