use regex::Regex;
use std::error::Error;

#[derive(Clone, Debug)]
pub struct Target {
    name: String,
    address: String,
    pattern: Option<Regex>,
}

impl Target {
    pub fn new<S: Into<String>>(name: S, address: S, pattern: Option<Regex>) -> Target {
        Target {
            name: name.into(),
            address: address.into(),
            pattern: pattern,
        }
    }

    pub fn address(&self) -> &str {
        self.address.as_ref()
    }
}

pub fn parse_target<S: Into<String>>(v: S) -> Result<Target, String> {
    // expected format is "name at target_url if regex_pattern"
    // only target_url is strictly required
    let literal_at = " at ";
    let literal_if = " if ";

    let mut name = None;
    let mut address = v.into();
    if let Some(at_pos) = address.find(literal_at) {
        name = Some(address[0..at_pos].into());
        address = address[at_pos + literal_at.len()..].into();
    }

    let mut pattern = None;
    if let Some(if_pos) = address.find(literal_if) {
        pattern = {
            let raw = &address[if_pos + literal_if.len()..];
            Some(Regex::new(raw).map_err(|e| {
                format!(
                    "The text '{}' after '{}' is not a valid regular expression: {}",
                    raw,
                    literal_if,
                    e.description()
                ).to_owned()
            })?)
        };

        address = address[0..if_pos].into();
    }

    Ok(Target::new(
        name.unwrap_or(address.clone()),
        address,
        pattern,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_with_everything_succeeds() {
        let t = parse_target("backend at http://127.0.0.1:9000 if ^/api.*$").unwrap();

        assert_eq!(t.name, "backend".to_owned());
        assert_eq!(t.address, "http://127.0.0.1:9000".to_owned());
        assert_eq!(t.pattern.unwrap().as_str(), "^/api.*$");
    }

    #[test]
    fn parse_target_with_no_name_succeeds() {
        let t = parse_target("http://127.0.0.1:9000 if ^/api.*$").unwrap();

        assert_eq!(t.name, "http://127.0.0.1:9000".to_owned());
        assert_eq!(t.address, "http://127.0.0.1:9000".to_owned());
        assert_eq!(t.pattern.unwrap().as_str(), "^/api.*$");
    }

    #[test]
    fn parse_target_with_no_pattern_succeeds() {
        let t = parse_target("backend at http://127.0.0.1:9000").unwrap();

        assert_eq!(t.name, "backend".to_owned());
        assert_eq!(t.address, "http://127.0.0.1:9000".to_owned());
        assert!(t.pattern.is_none());
    }

    #[test]
    fn parse_target_with_neither_name_nor_pattern_succeeds() {
        let t = parse_target("http://127.0.0.1:9000").unwrap();

        assert_eq!(t.name, "http://127.0.0.1:9000".to_owned());
        assert_eq!(t.address, "http://127.0.0.1:9000".to_owned());
        assert!(t.pattern.is_none());
    }

    #[test]
    fn parse_target_with_bad_regex_fails() {
        let e = parse_target("http://127.0.0.1:9000 if *invalid").unwrap_err();

        assert_eq!(
            e,
            "The text '*invalid' after ' if ' is not a valid regular expression: regex parse error:
    *invalid
    ^
error: repetition operator missing expression"
        );
    }
}
