use serde::{Deserialize, Serialize};

use crate::parse::{BlockKind, ByteRange};

pub use crate::id::BlockId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct BlockPatch {
    pub block_id: BlockId,
    pub parsed_kind: BlockKind,
    pub original_byte_range: Option<ByteRange>,
    pub edit: BlockEdit,
    pub dirty: bool,
    pub error: Option<BlockError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum BlockEdit {
    PreservedBytes,
    EditedBytes(String),
    SerializeFromTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
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
