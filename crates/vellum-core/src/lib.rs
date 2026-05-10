pub mod block;
pub mod fs;
pub mod hash;
pub mod id;
pub mod parse;
pub mod sidecar;
pub mod watch;

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert!(true);
    }
}
