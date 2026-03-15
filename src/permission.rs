use crate::types::PermissionRule;

/// Parse a `Bash(...)` rule string into a `PermissionRule`.
///
/// Supported formats:
/// - `Bash(ls *)` or `Bash(ls:*)` -> prefix "ls", wildcard
/// - `Bash(ls)` -> prefix "ls", exact
/// - `Bash(*)` -> prefix "", wildcard (matches everything)
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

    if expanded == "*" {
        return Some(PermissionRule {
            original: rule.to_string(),
            prefix: String::new(),
            is_wildcard: true,
        });
    }

    // Handle colon separator: `Bash(ls:*)` -> prefix "ls"
    if let Some(colon_pos) = expanded.find(':') {
        let prefix = expanded[..colon_pos].to_string();
        let suffix = &expanded[colon_pos + 1..];
        let is_wildcard = suffix.contains('*');
        return Some(PermissionRule {
            original: rule.to_string(),
            prefix,
            is_wildcard,
        });
    }

    // Handle trailing wildcard: `Bash(cargo add *)` -> prefix "cargo add"
    if expanded.ends_with(" *") {
        let prefix = expanded[..expanded.len() - 2].to_string();
        return Some(PermissionRule {
            original: rule.to_string(),
            prefix,
            is_wildcard: true,
        });
    }

    // Exact match: `Bash(ls)` -> prefix "ls"
    Some(PermissionRule {
        original: rule.to_string(),
        prefix: expanded,
        is_wildcard: false,
    })
}

/// Check if a command matches a permission rule.
pub fn matches(rule: &PermissionRule, command: &str) -> bool {
    let cmd = command.trim();

    // Wildcard with empty prefix matches everything
    if rule.prefix.is_empty() && rule.is_wildcard {
        return true;
    }

    if rule.is_wildcard {
        // Prefix match: command equals prefix, or starts with prefix followed by space or /
        cmd == rule.prefix
            || cmd.starts_with(&format!("{} ", rule.prefix))
            || cmd.starts_with(&format!("{}/", rule.prefix))
    } else {
        // Exact match
        cmd == rule.prefix
    }
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

    #[test]
    fn test_parse_bash_rule_with_wildcard() {
        let rule = parse_bash_rule("Bash(ls *)").unwrap();
        assert_eq!(rule.prefix, "ls");
        assert!(rule.is_wildcard);
    }

    #[test]
    fn test_parse_bash_rule_with_colon_wildcard() {
        let rule = parse_bash_rule("Bash(ls:*)").unwrap();
        assert_eq!(rule.prefix, "ls");
        assert!(rule.is_wildcard);
    }

    #[test]
    fn test_parse_bash_rule_exact() {
        let rule = parse_bash_rule("Bash(ls)").unwrap();
        assert_eq!(rule.prefix, "ls");
        assert!(!rule.is_wildcard);
    }

    #[test]
    fn test_parse_bash_rule_global_wildcard() {
        let rule = parse_bash_rule("Bash(*)").unwrap();
        assert_eq!(rule.prefix, "");
        assert!(rule.is_wildcard);
    }

    #[test]
    fn test_parse_non_bash_rule() {
        assert!(parse_bash_rule("Read(*)").is_none());
        assert!(parse_bash_rule("").is_none());
        assert!(parse_bash_rule("Bash()").is_none());
    }

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
    fn test_parse_bash_rule_subcommand_wildcard() {
        let rule = parse_bash_rule("Bash(cargo add *)").unwrap();
        assert_eq!(rule.prefix, "cargo add");
        assert!(rule.is_wildcard);
    }

    #[test]
    fn test_parse_bash_rule_subcommand_exact() {
        let rule = parse_bash_rule("Bash(gh pr merge --auto --squash)").unwrap();
        assert_eq!(rule.prefix, "gh pr merge --auto --squash");
        assert!(!rule.is_wildcard);
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
    fn test_parse_bash_rule_with_whitespace() {
        let rule = parse_bash_rule("  Bash(ls *)  ").unwrap();
        assert_eq!(rule.prefix, "ls");
        assert!(rule.is_wildcard);
    }

    #[test]
    fn test_matches_path_separator() {
        let rule = parse_bash_rule("Bash(ls *)").unwrap();
        assert!(matches(&rule, "ls/something"));
    }
}
