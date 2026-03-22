use crate::parser::{is_simple_command, parse_command};
use crate::permission::{matches as matches_rule_exact, matches_flexible};
use crate::types::{Judgment, PermissionRule, PermissionSet};

/// Judge a command against the permission set.
pub fn judge(command: &str, permissions: &PermissionSet, debug: bool, explain: bool) -> Judgment {
    if command.trim().is_empty() {
        return Judgment::Fallthrough("empty command".to_string());
    }

    if is_simple_command(command) {
        judge_simple(command, permissions, debug, explain)
    } else {
        judge_compound(command, permissions, debug, explain)
    }
}

/// Result of matching a rule against a command.
enum MatchKind {
    None,
    Exact,
    Flexible,
}

fn match_rule(rule: &PermissionRule, command: &str, flexible: bool) -> MatchKind {
    if matches_rule_exact(rule, command) {
        return MatchKind::Exact;
    }
    if flexible && matches_flexible(rule, command) {
        return MatchKind::Flexible;
    }
    MatchKind::None
}

fn judge_simple(
    command: &str,
    permissions: &PermissionSet,
    _debug: bool,
    explain: bool,
) -> Judgment {
    let cmd = command.trim();
    let flexible = permissions.flexible_match;

    // Check deny rules first
    for rule in &permissions.deny {
        let result = match_rule(rule, cmd, flexible);
        if !matches!(result, MatchKind::None) {
            let via = match result {
                MatchKind::Flexible => " [via flexible match]",
                _ => "",
            };
            if explain {
                eprintln!(
                    "[ccjanus] fallthrough: simple command '{cmd}' matches deny rule '{}'{via}",
                    rule.original
                );
            }
            return Judgment::Fallthrough(format!(
                "simple command matches deny rule: {}",
                rule.original
            ));
        }
    }

    // Check allow rules
    for rule in &permissions.allow {
        let result = match_rule(rule, cmd, flexible);
        if !matches!(result, MatchKind::None) {
            let via = match result {
                MatchKind::Flexible => " [via flexible match]",
                _ => "",
            };
            if explain {
                eprintln!(
                    "[ccjanus] allow: simple command '{cmd}' matches allow rule '{}'{via}",
                    rule.original
                );
            }
            return Judgment::Allow;
        }
    }

    if explain {
        eprintln!("[ccjanus] fallthrough: simple command '{cmd}' has no matching rule");
    }
    Judgment::Fallthrough(format!("no matching rule for: {cmd}"))
}

fn judge_compound(
    command: &str,
    permissions: &PermissionSet,
    debug: bool,
    explain: bool,
) -> Judgment {
    let segments = match parse_command(command) {
        Ok(s) => s,
        Err(e) => {
            if debug {
                eprintln!("[ccjanus] parse error: {e}");
            }
            return Judgment::Fallthrough(format!("parse error: {e}"));
        }
    };

    if segments.is_empty() {
        return Judgment::Fallthrough("no segments found".to_string());
    }

    let flexible = permissions.flexible_match;

    // Check deny rules against all segments
    for segment in &segments {
        for rule in &permissions.deny {
            let by_name = match_rule(rule, &segment.command_name, flexible);
            let by_text = match_rule(rule, &segment.full_text, flexible);
            let matched =
                !matches!(by_name, MatchKind::None) || !matches!(by_text, MatchKind::None);
            if matched {
                let via = if matches!(by_name, MatchKind::Flexible)
                    || matches!(by_text, MatchKind::Flexible)
                {
                    " [via flexible match]"
                } else {
                    ""
                };
                if explain {
                    eprintln!(
                        "[ccjanus] deny: segment '{}' matches deny rule '{}'{via}",
                        segment.full_text, rule.original
                    );
                }
                return Judgment::Deny(format!(
                    "segment '{}' matches deny rule: {}",
                    segment.full_text, rule.original
                ));
            }
        }
    }

    // Check if all segments are allowed
    let mut all_allowed = true;
    for segment in &segments {
        let segment_allowed = permissions.allow.iter().any(|rule| {
            !matches!(
                match_rule(rule, &segment.command_name, flexible),
                MatchKind::None
            ) || !matches!(
                match_rule(rule, &segment.full_text, flexible),
                MatchKind::None
            )
        });

        if !segment_allowed {
            if explain {
                eprintln!(
                    "[ccjanus] fallthrough: segment '{}' has no matching allow rule",
                    segment.full_text
                );
            }
            all_allowed = false;
            break;
        }
    }

    if all_allowed {
        if explain {
            eprintln!("[ccjanus] allow: all segments are allowed");
        }
        Judgment::Allow
    } else {
        Judgment::Fallthrough("not all segments are allowed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::parse_bash_rule;
    use crate::types::PermissionSet;

    fn make_permissions(allow: &[&str], deny: &[&str]) -> PermissionSet {
        PermissionSet {
            allow: allow.iter().filter_map(|s| parse_bash_rule(s)).collect(),
            deny: deny.iter().filter_map(|s| parse_bash_rule(s)).collect(),
            flexible_match: false,
        }
    }

    fn make_permissions_flexible(allow: &[&str], deny: &[&str]) -> PermissionSet {
        PermissionSet {
            allow: allow.iter().filter_map(|s| parse_bash_rule(s)).collect(),
            deny: deny.iter().filter_map(|s| parse_bash_rule(s)).collect(),
            flexible_match: true,
        }
    }

    #[test]
    fn test_simple_allow() {
        let perms = make_permissions(&["Bash(ls *)"], &[]);
        assert_eq!(judge("ls -la", &perms, false, false), Judgment::Allow);
    }

    #[test]
    fn test_simple_deny_fallthrough() {
        let perms = make_permissions(&[], &["Bash(rm *)"]);
        match judge("rm -rf /", &perms, false, false) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough, got {other:?}"),
        }
    }

    #[test]
    fn test_simple_no_match() {
        let perms = make_permissions(&["Bash(ls *)"], &[]);
        match judge("cat file", &perms, false, false) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough, got {other:?}"),
        }
    }

    #[test]
    fn test_compound_all_allowed() {
        let perms = make_permissions(&["Bash(ls *)", "Bash(grep *)"], &[]);
        assert_eq!(
            judge("ls /tmp | grep test", &perms, false, false),
            Judgment::Allow
        );
    }

    #[test]
    fn test_compound_partial_allowed() {
        let perms = make_permissions(&["Bash(ls *)"], &[]);
        match judge("ls /tmp | grep test", &perms, false, false) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough, got {other:?}"),
        }
    }

    #[test]
    fn test_compound_deny() {
        let perms = make_permissions(&["Bash(ls *)", "Bash(rm *)"], &["Bash(rm *)"]);
        match judge("ls /tmp | rm -rf /", &perms, false, false) {
            Judgment::Deny(_) => {}
            other => panic!("Expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn test_empty_command() {
        let perms = make_permissions(&["Bash(ls *)"], &[]);
        match judge("", &perms, false, false) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough, got {other:?}"),
        }
    }

    #[test]
    fn test_simple_deny_takes_priority() {
        let perms = make_permissions(&["Bash(rm *)"], &["Bash(rm *)"]);
        match judge("rm file", &perms, false, false) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough (deny on simple -> fallthrough), got {other:?}"),
        }
    }

    #[test]
    fn test_compound_three_commands() {
        let perms = make_permissions(&["Bash(cat *)", "Bash(sort *)", "Bash(head *)"], &[]);
        assert_eq!(
            judge("cat file | sort | head -5", &perms, false, false),
            Judgment::Allow
        );
    }

    #[test]
    fn test_compound_with_and() {
        let perms = make_permissions(&["Bash(ls *)", "Bash(echo *)"], &[]);
        assert_eq!(
            judge("ls && echo done", &perms, false, false),
            Judgment::Allow
        );
    }

    // --- flexible match tests ---

    #[test]
    fn test_flexible_simple_allow_with_options() {
        let perms = make_permissions_flexible(&["Bash(uv run ruff format *)"], &[]);
        assert_eq!(
            judge(
                "uv run --group dev ruff format chatbot-agent/",
                &perms,
                false,
                false
            ),
            Judgment::Allow
        );
    }

    #[test]
    fn test_flexible_simple_deny_with_options() {
        let perms = make_permissions_flexible(&["Bash(git *)"], &["Bash(git push * main)"]);
        match judge("git push --force origin main", &perms, false, false) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough, got {other:?}"),
        }
    }

    #[test]
    fn test_flexible_disabled_no_stripping() {
        let perms = make_permissions(&["Bash(uv run ruff format *)"], &[]);
        match judge(
            "uv run --group dev ruff format chatbot-agent/",
            &perms,
            false,
            false,
        ) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough without flexible match, got {other:?}"),
        }
    }

    // --- Tests: rules containing flag-like tokens ---

    #[test]
    fn test_flexible_rule_with_flag_matched_by_exact() {
        // Rule `python -m pytest *` contains `-m`.
        // match_rule tries matches() first which succeeds via prefix match.
        let perms = make_permissions_flexible(&["Bash(python -m pytest *)"], &[]);
        assert_eq!(
            judge("python -m pytest -v tests/", &perms, false, false),
            Judgment::Allow
        );
    }

    #[test]
    fn test_flexible_deny_rule_with_flag() {
        // Deny rule `git push --force *` contains `--force`.
        // matches() handles it via prefix match.
        let perms = make_permissions_flexible(&["Bash(git *)"], &["Bash(git push --force *)"]);
        match judge("git push --force origin main", &perms, false, false) {
            Judgment::Fallthrough(_) => {}
            other => panic!("Expected Fallthrough (deny matched via exact), got {other:?}"),
        }
    }

    #[test]
    fn test_flexible_rule_with_flag_no_extra_flags() {
        // Command matches rule exactly (no extra flags to strip).
        let perms = make_permissions_flexible(&["Bash(docker run --rm *)"], &[]);
        assert_eq!(
            judge("docker run --rm myimage", &perms, false, false),
            Judgment::Allow
        );
    }

    #[test]
    fn test_flexible_rule_with_flag_and_extra_flags() {
        // Rule: `git merge --squash *`, command has extra `--no-edit`.
        // matches() succeeds because "git merge --squash " is a prefix of the command.
        let perms = make_permissions_flexible(&["Bash(git merge --squash *)"], &[]);
        assert_eq!(
            judge(
                "git merge --squash --no-edit feature-branch",
                &perms,
                false,
                false
            ),
            Judgment::Allow
        );
    }
}
