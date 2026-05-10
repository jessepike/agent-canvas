use uuid::Uuid;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[ts(export, export_to = "../../../ui/src/types/generated/")]
#[ts(type = "string")]
pub struct BlockId(pub Uuid);

pub fn fresh() -> BlockId {
    BlockId(Uuid::new_v4())
}

impl From<Uuid> for BlockId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for BlockId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_ne!(fresh(), BlockId(Uuid::nil()));
    }
}
