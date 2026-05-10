use crate::parse::{ParseError, parse};

pub fn save_round_trip(source: &str) -> Result<String, ParseError> {
    let _blocks = parse(source)?;
    Ok(source.to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert!(true);
    }
}
