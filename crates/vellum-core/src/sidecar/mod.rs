use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

pub use crate::id::BlockId;
use crate::parse::BlockKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct IdentityMap {
    pub source_hash: [u8; 32],
    pub block_ids: Vec<BlockIdentity>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub base_snapshot: Option<BaseSnapshot>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub comments: Option<Vec<Comment>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct BaseSnapshot {
    pub hash: [u8; 32],
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct BlockIdentity {
    pub id: BlockId,
    pub byte_range_start: usize,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct Comment {
    pub id: String,
    pub author: String,
    #[ts(type = "number")]
    pub created_at: i64,
    pub anchor: CommentAnchor,
    pub body: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
#[serde(untagged)]
pub enum CommentAnchor {
    HtmlSelection(HtmlCommentAnchor),
    TextSelection(TextCommentAnchor),
    FileLevel(FileLevelAnchor),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct TextCommentAnchor {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<TextCommentAnchorKind>,
    pub block_id: Option<String>,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct HtmlCommentAnchor {
    pub kind: HtmlCommentAnchorKind,
    pub start_offset: usize,
    pub end_offset: usize,
    pub snapshot_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub struct FileLevelAnchor {
    pub kind: FileLevelKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum TextCommentAnchorKind {
    TextSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum HtmlCommentAnchorKind {
    HtmlSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
pub enum FileLevelKind {
    FileLevel,
}

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("walkdir error: {0}")]
    WalkDir(#[from] walkdir::Error),
    #[error("document path is outside vault root")]
    DocOutsideVault,
}

pub fn sidecar_path(vault_root: &Path, doc_path: &Path) -> PathBuf {
    let relative_doc_path = doc_path.strip_prefix(vault_root).unwrap_or(doc_path);
    vault_root
        .join(".vellum-cache")
        .join(relative_doc_path)
        .join("identity.json")
}

pub fn load_or_migrate(
    vault_root: &Path,
    doc_path: &Path,
    doc_source: &str,
) -> Result<Option<IdentityMap>, SidecarError> {
    let expected_path = sidecar_path(vault_root, doc_path);
    if expected_path.exists() {
        return read_identity(&expected_path).map(Some);
    }

    let source_hash = *blake3::hash(doc_source.as_bytes()).as_bytes();
    let cache_root = vault_root.join(".vellum-cache");
    if !cache_root.exists() {
        return Ok(None);
    }

    for entry in WalkDir::new(&cache_root) {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "identity.json" {
            continue;
        }

        let candidate = read_identity(entry.path())?;
        if candidate.source_hash != source_hash {
            continue;
        }

        migrate_sidecar_dir(entry.path(), &expected_path)?;
        return Ok(Some(candidate));
    }

    Ok(None)
}

pub fn save(
    vault_root: &Path,
    doc_path: &Path,
    identity: &IdentityMap,
) -> Result<(), SidecarError> {
    let path = sidecar_path(vault_root, doc_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(identity)?)?;
    Ok(())
}

fn read_identity(path: &Path) -> Result<IdentityMap, SidecarError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn migrate_sidecar_dir(old_identity_path: &Path, expected_path: &Path) -> Result<(), SidecarError> {
    let old_dir = old_identity_path
        .parent()
        .ok_or(SidecarError::DocOutsideVault)?;
    let expected_dir = expected_path
        .parent()
        .ok_or(SidecarError::DocOutsideVault)?;
    if let Some(expected_parent) = expected_dir.parent() {
        fs::create_dir_all(expected_parent)?;
    }
    fs::rename(old_dir, expected_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{id, parse::BlockKind};
    use tempfile::TempDir;

    #[test]
    fn sidecar_load_returns_none_when_missing() {
        let vault = TempDir::new().unwrap();
        let doc = vault.path().join("doc.md");

        let identity = load_or_migrate(vault.path(), &doc, "# Hello\n").unwrap();

        assert_eq!(identity, None);
    }

    #[test]
    fn sidecar_load_returns_existing() {
        let vault = TempDir::new().unwrap();
        let doc = vault.path().join("doc.md");
        let expected = sample_identity("# Hello\n");
        save(vault.path(), &doc, &expected).unwrap();

        let actual = load_or_migrate(vault.path(), &doc, "# Hello\n").unwrap();

        assert_eq!(actual, Some(expected));
    }

    #[test]
    fn sidecar_migrates_from_old_path() {
        let vault = TempDir::new().unwrap();
        let old_doc = vault.path().join("old.md");
        let new_doc = vault.path().join("nested").join("new.md");
        let expected = sample_identity("# Hello\n");
        save(vault.path(), &old_doc, &expected).unwrap();

        let actual = load_or_migrate(vault.path(), &new_doc, "# Hello\n").unwrap();

        assert_eq!(actual, Some(expected));
        assert!(!sidecar_path(vault.path(), &old_doc).exists());
        assert!(sidecar_path(vault.path(), &new_doc).exists());
    }

    #[test]
    fn sidecar_save_creates_parent_dirs() {
        let vault = TempDir::new().unwrap();
        let doc = vault.path().join("deep").join("path").join("doc.md");
        let identity = sample_identity("# Hello\n");

        save(vault.path(), &doc, &identity).unwrap();

        assert!(sidecar_path(vault.path(), &doc).exists());
    }

    #[test]
    fn file_level_comment_anchor_round_trips() {
        let raw = r#"{ "kind": "file_level" }"#;

        let anchor: CommentAnchor = serde_json::from_str(raw).expect("file-level anchor");
        assert_eq!(
            anchor,
            CommentAnchor::FileLevel(FileLevelAnchor {
                kind: FileLevelKind::FileLevel
            })
        );

        let encoded = serde_json::to_value(anchor).expect("anchor json");
        assert_eq!(encoded["kind"], "file_level");
    }

    #[test]
    fn legacy_markdown_comment_anchor_still_deserializes() {
        let raw = r#"{
          "source_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
          "block_ids": [],
          "comments": [{
            "id": "00000000-0000-4000-8000-000000000000",
            "author": "jesse",
            "created_at": 1,
            "anchor": { "block_id": null, "start_offset": 3, "end_offset": 9 },
            "body": "legacy",
            "resolved": false
          }]
        }"#;

        let identity: IdentityMap = serde_json::from_str(raw).expect("legacy identity");
        let comments = identity.comments.expect("comments");
        assert!(matches!(
            comments[0].anchor,
            CommentAnchor::TextSelection(TextCommentAnchor {
                kind: None,
                block_id: None,
                start_offset: 3,
                end_offset: 9
            })
        ));
    }

    fn sample_identity(source: &str) -> IdentityMap {
        IdentityMap {
            source_hash: *blake3::hash(source.as_bytes()).as_bytes(),
            block_ids: vec![BlockIdentity {
                id: id::fresh(),
                byte_range_start: 0,
                kind: BlockKind::Heading,
            }],
            base_snapshot: None,
            comments: None,
        }
    }
}
