use uuid::Uuid;

pub fn new_block_id() -> Uuid {
    Uuid::new_v4()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_ne!(new_block_id(), Uuid::nil());
    }
}
