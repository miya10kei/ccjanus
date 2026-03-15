use crate::parser::{is_simple_command, parse_command};
use crate::permission::matches;
use crate::types::{Judgment, PermissionSet};

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

fn judge_simple(
    command: &str,
    permissions: &PermissionSet,
    _debug: bool,
    explain: bool,
) -> Judgment {
    let cmd = command.trim();

    // Check deny rules first
    for rule in &permissions.deny {
        if matches(rule, cmd) {
            if explain {
                eprintln!(
                    "[ccjanus] fallthrough: simple command '{cmd}' matches deny rule '{}'",
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
        if matches(rule, cmd) {
            if explain {
                eprintln!(
                    "[ccjanus] allow: simple command '{cmd}' matches allow rule '{}'",
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

    // Check deny rules against all segments
    for segment in &segments {
        for rule in &permissions.deny {
            if matches(rule, &segment.command_name) || matches(rule, &segment.full_text) {
                if explain {
                    eprintln!(
                        "[ccjanus] deny: segment '{}' matches deny rule '{}'",
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
        let segment_allowed = permissions
            .allow
            .iter()
            .any(|rule| matches(rule, &segment.command_name) || matches(rule, &segment.full_text));

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
}
