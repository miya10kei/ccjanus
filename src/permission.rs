use crate::types::PermissionRule;

/// Parse a `Bash(...)` rule string into a `PermissionRule`.
///
/// Supported formats:
/// - `Bash(ls *)` -> segments `["ls ", ""]` (trailing wildcard)
/// - `Bash(ls:*)` -> segments `["ls ", ""]` (colon separator, converted)
/// - `Bash(ls)` -> segments `["ls"]` (exact match)
/// - `Bash(*)` -> segments `["", ""]` (matches everything)
/// - `Bash(git push * main)` -> segments `["git push ", " main"]` (interior wildcard)
/// - `Bash(* --version)` -> segments `["", " --version"]` (leading wildcard)
/// - `Bash(git * * main)` -> segments `["git ", " ", " main"]` (multiple wildcards)
pub fn parse_bash_rule(rule: &str) -> Option<PermissionRule> {
    let trimmed = rule.trim();
    if !trimmed.starts_with("Bash(") || !trimmed.ends_with(')') {
        return None;
    }

    let inner = &trimmed[5..trimmed.len() - 1];
    if inner.is_empty() {
        return None;
    }

    let expanded = expand_home(inner);

    // Handle colon separator: `Bash(ls:*)` -> convert to `ls *` then split
    let normalized = if let Some(colon_pos) = expanded.find(':') {
        let prefix = &expanded[..colon_pos];
        let suffix = &expanded[colon_pos + 1..];
        format!("{prefix} {suffix}")
    } else {
        expanded
    };

    let segments: Vec<String> = normalized.split('*').map(|s| s.to_string()).collect();

    Some(PermissionRule {
        original: rule.to_string(),
        segments,
    })
}

/// Check if a command matches a permission rule.
pub fn matches(rule: &PermissionRule, command: &str) -> bool {
    let cmd = command.trim();

    if rule.segments.len() == 1 {
        // Exact match
        return cmd == rule.segments[0];
    }

    // Pattern match with wildcards
    let first = &rule.segments[0];
    let last = &rule.segments[rule.segments.len() - 1];

    // Trailing wildcard backward compatibility:
    // `["prefix ", ""]` should also match `cmd == prefix` or `cmd.starts_with(prefix + "/")`
    let is_trailing_wildcard = last.is_empty() && rule.segments.len() == 2;

    if is_trailing_wildcard {
        let prefix = first.trim_end();
        if prefix.is_empty() {
            // Global wildcard: `["", ""]` matches everything
            return true;
        }
        // Match: exact prefix, prefix + space..., or prefix + /...
        if cmd == prefix
            || cmd.starts_with(&format!("{prefix} "))
            || cmd.starts_with(&format!("{prefix}/"))
        {
            return true;
        }
        return false;
    }

    // General wildcard matching
    // Check first segment: command must start with it
    if !cmd.starts_with(first.as_str()) {
        return false;
    }

    // Check last segment: command must end with it
    if !cmd.ends_with(last.as_str()) {
        return false;
    }

    // Check middle segments appear in order
    let mut pos = first.len();
    for segment in &rule.segments[1..rule.segments.len() - 1] {
        let remaining = &cmd[pos..cmd.len() - last.len()];
        match remaining.find(segment.as_str()) {
            Some(found) => pos += found + segment.len(),
            None => return false,
        }
    }

    // Ensure there's room for the last segment
    pos <= cmd.len() - last.len()
}

fn expand_home(s: &str) -> String {
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if home.is_empty() {
        return s.to_string();
    }

    s.replace("~/", &format!("{home}/"))
        .replace("$HOME/", &format!("{home}/"))
        .replace("${HOME}/", &format!("{home}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Parse tests ---

    #[test]
    fn test_parse_bash_rule_with_wildcard() {
        let rule = parse_bash_rule("Bash(ls *)").unwrap();
        assert_eq!(rule.segments, vec!["ls ", ""]);
    }

    #[test]
    fn test_parse_bash_rule_with_colon_wildcard() {
        let rule = parse_bash_rule("Bash(ls:*)").unwrap();
        assert_eq!(rule.segments, vec!["ls ", ""]);
    }

    #[test]
    fn test_parse_bash_rule_exact() {
        let rule = parse_bash_rule("Bash(ls)").unwrap();
        assert_eq!(rule.segments, vec!["ls"]);
    }

    #[test]
    fn test_parse_bash_rule_global_wildcard() {
        let rule = parse_bash_rule("Bash(*)").unwrap();
        assert_eq!(rule.segments, vec!["", ""]);
    }

    #[test]
    fn test_parse_non_bash_rule() {
        assert!(parse_bash_rule("Read(*)").is_none());
        assert!(parse_bash_rule("").is_none());
        assert!(parse_bash_rule("Bash()").is_none());
    }

    #[test]
    fn test_parse_bash_rule_subcommand_wildcard() {
        let rule = parse_bash_rule("Bash(cargo add *)").unwrap();
        assert_eq!(rule.segments, vec!["cargo add ", ""]);
    }

    #[test]
    fn test_parse_bash_rule_subcommand_exact() {
        let rule = parse_bash_rule("Bash(gh pr merge --auto --squash)").unwrap();
        assert_eq!(rule.segments, vec!["gh pr merge --auto --squash"]);
    }

    #[test]
    fn test_parse_bash_rule_with_whitespace() {
        let rule = parse_bash_rule("  Bash(ls *)  ").unwrap();
        assert_eq!(rule.segments, vec!["ls ", ""]);
    }

    #[test]
    fn test_parse_bash_rule_interior_wildcard() {
        let rule = parse_bash_rule("Bash(git push * main)").unwrap();
        assert_eq!(rule.segments, vec!["git push ", " main"]);
    }

    #[test]
    fn test_parse_bash_rule_leading_wildcard() {
        let rule = parse_bash_rule("Bash(* --version)").unwrap();
        assert_eq!(rule.segments, vec!["", " --version"]);
    }

    #[test]
    fn test_parse_bash_rule_multiple_wildcards() {
        let rule = parse_bash_rule("Bash(git * * main)").unwrap();
        assert_eq!(rule.segments, vec!["git ", " ", " main"]);
    }

    // --- Match tests ---

    #[test]
    fn test_matches_wildcard() {
        let rule = parse_bash_rule("Bash(ls *)").unwrap();
        assert!(matches(&rule, "ls"));
        assert!(matches(&rule, "ls -la"));
        assert!(matches(&rule, "ls /tmp"));
        assert!(!matches(&rule, "cat file"));
    }

    #[test]
    fn test_matches_exact() {
        let rule = parse_bash_rule("Bash(ls)").unwrap();
        assert!(matches(&rule, "ls"));
        assert!(!matches(&rule, "ls -la"));
    }

    #[test]
    fn test_matches_global_wildcard() {
        let rule = parse_bash_rule("Bash(*)").unwrap();
        assert!(matches(&rule, "ls"));
        assert!(matches(&rule, "rm -rf /"));
        assert!(matches(&rule, "anything"));
    }

    #[test]
    fn test_matches_subcommand_wildcard() {
        let rule = parse_bash_rule("Bash(cargo add *)").unwrap();
        assert!(matches(&rule, "cargo add serde"));
        assert!(matches(&rule, "cargo add"));
        assert!(!matches(&rule, "cargo build"));
        assert!(!matches(&rule, "cargo"));
    }

    #[test]
    fn test_matches_path_separator() {
        let rule = parse_bash_rule("Bash(ls *)").unwrap();
        assert!(matches(&rule, "ls/something"));
    }

    #[test]
    fn test_matches_interior_wildcard() {
        let rule = parse_bash_rule("Bash(git push * main)").unwrap();
        assert!(matches(&rule, "git push origin main"));
        assert!(!matches(&rule, "git push origin develop"));
    }

    #[test]
    fn test_matches_leading_wildcard() {
        let rule = parse_bash_rule("Bash(* --version)").unwrap();
        assert!(matches(&rule, "node --version"));
        assert!(matches(&rule, "python --version"));
        assert!(!matches(&rule, "node --help"));
    }

    #[test]
    fn test_matches_multiple_wildcards() {
        let rule = parse_bash_rule("Bash(git * * main)").unwrap();
        assert!(matches(&rule, "git push origin main"));
        assert!(!matches(&rule, "git push origin develop"));
    }

    #[test]
    fn test_matches_interior_wildcard_no_match() {
        let rule = parse_bash_rule("Bash(docker run * --rm)").unwrap();
        assert!(matches(&rule, "docker run myimage --rm"));
        assert!(!matches(&rule, "docker run myimage --detach"));
        assert!(!matches(&rule, "podman run myimage --rm"));
    }

    #[test]
    fn test_matches_overlapping_segments() {
        let rule = parse_bash_rule("Bash(echo * end * end)").unwrap();
        assert!(matches(&rule, "echo hello end world end"));
        assert!(!matches(&rule, "echo hello end world"));
    }

    #[test]
    fn test_matches_consecutive_wildcards() {
        let rule = parse_bash_rule("Bash(git ** main)").unwrap();
        assert_eq!(rule.segments, vec!["git ", "", " main"]);
        assert!(matches(&rule, "git push origin main"));
        assert!(!matches(&rule, "git push origin develop"));
    }
}
