pub fn content_hash(bytes: &[u8]) -> blake3::Hash {
    blake3::hash(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let hash = content_hash(b"vellum");
        assert_eq!(hash.as_bytes().len(), 32);
    }
}
