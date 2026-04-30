//! `daylog readme` — print the embedded README.md to stdout.
//!
//! The README is `include_str!`-embedded at compile time so the
//! installed binary (`~/.cargo/bin/daylog`) ships with its own docs —
//! no separate clone or network access required for an AI agent or
//! user to discover how to use the tool.

const README: &str = include_str!("../../README.md");

pub fn execute() {
    print!("{README}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_is_embedded_and_non_empty() {
        assert!(!README.is_empty(), "README must be embedded");
    }

    #[test]
    fn readme_mentions_daylog_food() {
        assert!(
            README.contains("daylog food"),
            "embedded README should describe the food subcommand"
        );
    }
}
