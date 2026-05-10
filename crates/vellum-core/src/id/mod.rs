use uuid::Uuid;

pub type BlockId = Uuid;

pub fn fresh() -> BlockId {
    Uuid::new_v4()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_ne!(fresh(), Uuid::nil());
    }
}
