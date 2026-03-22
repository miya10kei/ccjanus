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

/// Check if a command matches a permission rule with option-stripping.
///
/// Tries all possible interpretations of option-like arguments (`--flag value`, `-x`, etc.)
/// by recursively exploring both "consume value" and "don't consume value" branches.
///
/// Note: This does NOT call `matches()` first. Callers should check `matches()` for
/// exact matching before calling this function to avoid redundant work.
pub fn matches_flexible(rule: &PermissionRule, command: &str) -> bool {
    // Don't apply option stripping for exact-match rules (no wildcards)
    if rule.segments.len() <= 1 {
        return false;
    }

    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut current = Vec::new();
    let mut calls = 0;
    matches_stripped(rule, &tokens, 0, &mut current, &mut calls)
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

/// Maximum number of recursive calls to prevent exponential blowup on adversarial inputs.
const FLEXIBLE_MATCH_RECURSION_LIMIT: usize = 256;

/// Recursively try all possible option-stripping interpretations and check for a match.
///
/// For each flag token followed by a non-flag token, explores two branches:
/// - The next token is the flag's value → skip both
/// - The next token is a positional argument → skip only the flag
///
/// This avoids the greedy heuristic problem where boolean flags like `--verbose`
/// would incorrectly consume the next positional argument.
fn matches_stripped<'a>(
    rule: &PermissionRule,
    tokens: &[&'a str],
    i: usize,
    current: &mut Vec<&'a str>,
    calls: &mut usize,
) -> bool {
    *calls += 1;
    if *calls > FLEXIBLE_MATCH_RECURSION_LIMIT {
        return false;
    }

    if i >= tokens.len() {
        let stripped = current.join(" ");
        return matches(rule, &stripped);
    }

    let token = tokens[i];

    if token == "--" {
        // Everything after `--` is positional
        let saved_len = current.len();
        current.extend_from_slice(&tokens[i + 1..]);
        let result = matches(rule, &current.join(" "));
        current.truncate(saved_len);
        return result;
    }

    if token.starts_with("--") {
        if token.contains('=') {
            // `--flag=value`: always skip as single token
            return matches_stripped(rule, tokens, i + 1, current, calls);
        }
        if i + 1 < tokens.len() && !tokens[i + 1].starts_with('-') {
            // `--flag` followed by non-flag: try both interpretations
            // Branch A: consume next token as value (skip both)
            if matches_stripped(rule, tokens, i + 2, current, calls) {
                return true;
            }
            // Branch B: flag is boolean, next token is positional (skip only flag)
            return matches_stripped(rule, tokens, i + 1, current, calls);
        }
        // `--flag` at end or followed by another flag: skip only flag
        return matches_stripped(rule, tokens, i + 1, current, calls);
    }

    if token.starts_with('-') && token.len() > 1 && token.as_bytes()[1].is_ascii_alphabetic() {
        if token.len() == 2 {
            // `-x`: single-char short option
            if i + 1 < tokens.len() && !tokens[i + 1].starts_with('-') {
                // Try both: consume next as value, or treat as boolean
                if matches_stripped(rule, tokens, i + 2, current, calls) {
                    return true;
                }
                return matches_stripped(rule, tokens, i + 1, current, calls);
            }
        }
        // `-xvalue` or `-x` at end: skip this token
        return matches_stripped(rule, tokens, i + 1, current, calls);
    }

    // Positional argument: keep
    current.push(token);
    let result = matches_stripped(rule, tokens, i + 1, current, calls);
    current.pop();
    result
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

    // --- matches_flexible tests ---

    #[test]
    fn test_matches_flexible_with_options() {
        let rule = parse_bash_rule("Bash(uv run ruff format *)").unwrap();
        assert!(!matches(
            &rule,
            "uv run --group dev ruff format chatbot-agent/"
        ));
        assert!(matches_flexible(
            &rule,
            "uv run --group dev ruff format chatbot-agent/"
        ));
    }

    #[test]
    fn test_matches_flexible_without_options() {
        let rule = parse_bash_rule("Bash(uv run ruff format *)").unwrap();
        assert!(matches_flexible(&rule, "uv run ruff format chatbot-agent/"));
    }

    #[test]
    fn test_matches_flexible_exact_rule_no_stripping() {
        let rule = parse_bash_rule("Bash(gh pr merge --auto --squash)").unwrap();
        // Exact rules (no wildcards) always return false from matches_flexible
        assert!(!matches_flexible(&rule, "gh pr merge --auto --squash"));
        assert!(!matches_flexible(&rule, "gh pr merge"));
        // They should be matched by matches() directly
        assert!(matches(&rule, "gh pr merge --auto --squash"));
    }

    #[test]
    fn test_matches_flexible_no_match() {
        let rule = parse_bash_rule("Bash(cargo build *)").unwrap();
        assert!(!matches_flexible(
            &rule,
            "uv run --group dev ruff format file.py"
        ));
    }

    #[test]
    fn test_matches_flexible_interior_wildcard_with_options() {
        let rule = parse_bash_rule("Bash(git push * main)").unwrap();
        assert!(matches_flexible(&rule, "git push --force origin main"));
    }

    #[test]
    fn test_matches_flexible_boolean_flag_does_not_consume_positional() {
        // --verbose is boolean, ruff is positional: both interpretations are tried
        let rule = parse_bash_rule("Bash(uv run ruff format *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "uv run --group dev --verbose ruff format file.py"
        ));
    }

    #[test]
    fn test_matches_flexible_long_flag_with_equals() {
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(matches_flexible(&rule, "cmd --config=file.toml run arg"));
    }

    #[test]
    fn test_matches_flexible_short_flag_with_value() {
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(matches_flexible(&rule, "cmd -g dev run arg"));
    }

    #[test]
    fn test_matches_flexible_short_flag_combined() {
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(matches_flexible(&rule, "cmd -xvf run arg"));
    }

    #[test]
    fn test_matches_flexible_double_dash_separator() {
        // After `--`, everything is kept as positional
        let rule = parse_bash_rule("Bash(cmd --not-a-flag *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "cmd --flag val -- --not-a-flag positional"
        ));
    }

    #[test]
    fn test_matches_flexible_numeric_argument_preserved() {
        let rule = parse_bash_rule("Bash(head *)").unwrap();
        // -5 starts with non-alpha after -, so it's kept as positional
        assert!(matches_flexible(&rule, "head -5 file.txt"));
    }

    #[test]
    fn test_matches_flexible_flag_at_end() {
        let rule = parse_bash_rule("Bash(cmd arg *)").unwrap();
        assert!(matches_flexible(&rule, "cmd arg --verbose"));
    }

    #[test]
    fn test_matches_flexible_consecutive_flags() {
        let rule = parse_bash_rule("Bash(cmd arg *)").unwrap();
        assert!(matches_flexible(&rule, "cmd --flag1 --flag2 arg"));
    }

    // --- Additional edge case tests for backtracking ---

    #[test]
    fn test_matches_flexible_multiple_boolean_flags_before_positional() {
        // cargo build --release --verbose src/
        let rule = parse_bash_rule("Bash(cargo build *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "cargo --release --verbose build src/"
        ));
    }

    #[test]
    fn test_matches_flexible_mixed_flag_styles() {
        // pytest -v --timeout 30 --no-header tests/
        let rule = parse_bash_rule("Bash(pytest *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "pytest -v --timeout 30 --no-header tests/"
        ));
    }

    #[test]
    fn test_matches_flexible_npm_run_with_flags() {
        let rule = parse_bash_rule("Bash(npm run build *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "npm --prefix ./app run build --production"
        ));
    }

    #[test]
    fn test_matches_flexible_docker_compose() {
        let rule = parse_bash_rule("Bash(docker compose up *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "docker compose -f docker-compose.yml up -d service"
        ));
    }

    #[test]
    fn test_matches_flexible_kubectl_apply() {
        let rule = parse_bash_rule("Bash(kubectl apply *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "kubectl --context staging -n prod apply -f deployment.yaml"
        ));
    }

    #[test]
    fn test_matches_flexible_many_boolean_flags() {
        // All boolean flags: none should consume the positional 'start'
        let rule = parse_bash_rule("Bash(app start *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "app --debug --verbose --dry-run --force start server"
        ));
    }

    #[test]
    fn test_matches_flexible_flag_value_then_boolean_then_positional() {
        // --output file is a flag+value, --verbose is boolean, src/ is positional
        let rule = parse_bash_rule("Bash(compile src/ *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "compile --output file --verbose src/ main.c"
        ));
    }

    #[test]
    fn test_matches_flexible_short_flag_value_before_subcommand() {
        // -C path is a flag+value, then subcommand 'status'
        let rule = parse_bash_rule("Bash(git status *)").unwrap();
        assert!(matches_flexible(&rule, "git -C /repo status --short"));
    }

    #[test]
    fn test_matches_flexible_wrong_command_still_rejected() {
        // Even with flexible match, totally different command should not match
        let rule = parse_bash_rule("Bash(cargo test *)").unwrap();
        assert!(!matches_flexible(
            &rule,
            "npm --verbose run build --production"
        ));
    }

    #[test]
    fn test_matches_flexible_deny_not_bypassed_by_flag_insertion() {
        // Adding flags should not bypass deny rules
        let rule = parse_bash_rule("Bash(rm *)").unwrap();
        assert!(matches_flexible(&rule, "rm --force --recursive /important"));
    }

    #[test]
    fn test_matches_flexible_python_module_exec() {
        // Rule contains `-m pytest` which includes a flag-like token.
        // matches_flexible strips `-m` from the command, so it can't match.
        // This should be matched by matches() (exact match) instead.
        let rule = parse_bash_rule("Bash(python -m pytest *)").unwrap();
        assert!(matches(&rule, "python -m pytest -v tests/"));
        // matches_flexible alone cannot match because -m gets stripped
        assert!(!matches_flexible(&rule, "python -m pytest -v tests/"));
    }

    // --- Stress / deep backtracking tests ---

    #[test]
    fn test_matches_flexible_deep_backtracking() {
        // 5 ambiguous flags before positional: 2^5 = 32 paths
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "cmd --a x --b y --c z --d w --e v run target"
        ));
    }

    #[test]
    fn test_matches_flexible_all_flags_are_boolean() {
        // Every flag is boolean, so all following tokens are positional
        let rule = parse_bash_rule("Bash(cmd a b c *)").unwrap();
        assert!(matches_flexible(&rule, "cmd --x a --y b --z c rest"));
    }

    #[test]
    fn test_matches_flexible_all_flags_take_values() {
        // Every flag consumes the next token
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "cmd --user admin --pass secret --env prod run deploy"
        ));
    }

    #[test]
    fn test_matches_flexible_positional_looks_like_flag_value() {
        // 'dev' could be --group's value or a positional arg
        // Only the "consume" interpretation works here
        let rule = parse_bash_rule("Bash(cmd ruff *)").unwrap();
        assert!(matches_flexible(&rule, "cmd --group dev ruff check"));
    }

    #[test]
    fn test_matches_flexible_no_positional_args_after_stripping() {
        // All non-flag tokens get consumed as flag values → nothing left
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(!matches_flexible(&rule, "--flag1 val1 --flag2 val2"));
    }

    #[test]
    fn test_matches_flexible_single_token_command() {
        let rule = parse_bash_rule("Bash(ls *)").unwrap();
        assert!(matches_flexible(&rule, "ls"));
    }

    #[test]
    fn test_matches_flexible_only_flags() {
        // Command is only flags, rule expects a positional prefix
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(!matches_flexible(&rule, "--verbose --debug"));
    }

    #[test]
    fn test_matches_flexible_flag_between_subcommands() {
        // Flag sits between two subcommands that the rule expects in sequence
        let rule = parse_bash_rule("Bash(git remote add *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "git remote --verbose add origin https://example.com"
        ));
    }

    #[test]
    fn test_matches_flexible_repeated_flag_names() {
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "cmd --env staging --env prod run server"
        ));
    }

    #[test]
    fn test_matches_flexible_equals_flag_mixed_with_space_flag() {
        let rule = parse_bash_rule("Bash(terraform apply *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "terraform -chdir=infra --auto-approve --var region=us-east-1 apply main.tf"
        ));
    }

    #[test]
    fn test_matches_flexible_interior_wildcard_with_multiple_flags() {
        let rule = parse_bash_rule("Bash(git push * main)").unwrap();
        assert!(matches_flexible(
            &rule,
            "git push --force --no-verify --set-upstream origin main"
        ));
    }

    #[test]
    fn test_matches_flexible_trailing_flags_after_positional() {
        let rule = parse_bash_rule("Bash(cargo test *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "cargo test my_module --no-fail-fast -- --nocapture"
        ));
    }

    #[test]
    fn test_matches_flexible_path_with_dash_prefix() {
        // File named "-important" should be kept as positional (starts with - but non-alpha)
        let rule = parse_bash_rule("Bash(cat *)").unwrap();
        assert!(matches_flexible(&rule, "cat -important.txt"));
        // But if it starts with -[alpha], it's treated as a flag
        // "cat -f" → -f is stripped, leaving "cat" which matches "cat *"
        assert!(matches_flexible(&rule, "cat -f"));
    }

    #[test]
    fn test_matches_flexible_empty_after_double_dash() {
        // Nothing after -- means no extra positionals
        let rule = parse_bash_rule("Bash(cmd run *)").unwrap();
        assert!(matches_flexible(&rule, "cmd --flag val run --"));
    }

    #[test]
    fn test_matches_flexible_rule_with_path() {
        let rule = parse_bash_rule("Bash(uv run ruff check src/ *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "uv run --group dev ruff check src/ tests/"
        ));
    }

    #[test]
    fn test_matches_flexible_multiple_short_flags_each_with_value() {
        let rule = parse_bash_rule("Bash(ffmpeg output.mp4 *)").unwrap();
        assert!(matches_flexible(
            &rule,
            "ffmpeg -i input.mp4 -c copy -y output.mp4 -v quiet"
        ));
    }

    #[test]
    fn test_matches_flexible_mismatched_positional_order() {
        // Even with stripping, positional order must match the rule
        let rule = parse_bash_rule("Bash(cmd build deploy *)").unwrap();
        // "deploy build" is wrong order
        assert!(!matches_flexible(&rule, "cmd deploy --flag build rest"));
    }

    // --- Tests for rules containing flag-like tokens ---

    #[test]
    fn test_matches_flexible_rule_with_short_flag_matches_via_exact() {
        // Rule: `python -m pytest *` contains `-m` which looks like a flag.
        // matches() handles this via prefix match (no stripping).
        // matches_flexible strips `-m` from the command, so it fails alone.
        let rule = parse_bash_rule("Bash(python -m pytest *)").unwrap();
        assert!(matches(&rule, "python -m pytest tests/"));
        assert!(matches(&rule, "python -m pytest -v tests/"));
        assert!(!matches_flexible(&rule, "python -m pytest tests/"));
    }

    #[test]
    fn test_matches_flexible_rule_with_long_flag_matches_via_exact() {
        // Rule contains `--squash` as part of the pattern.
        let rule = parse_bash_rule("Bash(git merge --squash *)").unwrap();
        assert!(matches(&rule, "git merge --squash feature-branch"));
        // matches_flexible strips --squash from the command
        assert!(!matches_flexible(
            &rule,
            "git merge --squash feature-branch"
        ));
    }

    #[test]
    fn test_matches_flexible_rule_with_flag_and_extra_flags_in_command() {
        // Rule has `--squash`, command has `--squash` + extra `--no-edit`.
        // matches() handles this because prefix "git merge --squash " matches.
        let rule = parse_bash_rule("Bash(git merge --squash *)").unwrap();
        assert!(matches(
            &rule,
            "git merge --squash --no-edit feature-branch"
        ));
    }

    #[test]
    fn test_matches_flexible_rule_with_flag_in_interior_wildcard() {
        // Rule: `docker run * --rm` — the flag `--rm` is at the END of the rule.
        // matches() checks if command ends with " --rm" (interior wildcard).
        let rule = parse_bash_rule("Bash(docker run * --rm)").unwrap();
        assert!(matches(&rule, "docker run myimage --rm"));
        // With flexible match, extra flags before `--rm` still work via matches()
        assert!(matches(&rule, "docker run -d --name web myimage --rm"));
        // matches_flexible can also find this by stripping -d and --name web,
        // leaving "docker run myimage --rm" (if --rm is kept as positional...
        // but --rm starts with --, so it gets stripped too!)
        // This means matches_flexible alone cannot reliably match rules ending with flags.
        assert!(!matches_flexible(
            &rule,
            "docker run -d --name web myimage --rm"
        ));
    }

    #[test]
    fn test_matches_flexible_rule_with_leading_wildcard_flag() {
        // Rule: `* --version` — expects command to end with ` --version`.
        let rule = parse_bash_rule("Bash(* --version)").unwrap();
        assert!(matches(&rule, "node --version"));
        // matches_flexible strips --version from the command!
        assert!(!matches_flexible(&rule, "node --version"));
    }

    #[test]
    fn test_matches_flexible_rule_with_flag_equals_value() {
        // Rule contains `--format=json` as literal text.
        let rule = parse_bash_rule("Bash(kubectl get pods --format=json *)").unwrap();
        assert!(matches(&rule, "kubectl get pods --format=json -n prod"));
        // matches_flexible strips --format=json from command
        assert!(!matches_flexible(
            &rule,
            "kubectl get pods --format=json -n prod"
        ));
    }

    #[test]
    fn test_judge_rule_with_flag_matched_by_exact_then_flexible_not_needed() {
        // End-to-end: match_rule tries matches() first, which handles rule flags.
        // This confirms the full pipeline works for rules with flags.
        let rule = parse_bash_rule("Bash(python -m pytest *)").unwrap();
        // matches() succeeds → MatchKind::Exact, no need for flexible
        assert!(matches(&rule, "python -m pytest -v --timeout 30 tests/"));
    }

    #[test]
    fn test_matches_flexible_rule_with_combined_short_flag() {
        // Rule contains `-rf` as combined short flags (like rm -rf).
        let rule = parse_bash_rule("Bash(rm -rf *)").unwrap();
        assert!(matches(&rule, "rm -rf /tmp/old"));
        // matches_flexible strips -rf from command
        assert!(!matches_flexible(&rule, "rm -rf /tmp/old"));
    }

    #[test]
    fn test_matches_flexible_recursion_limit() {
        // Rule expects "nonexistent" which doesn't appear in the command.
        // Without the limit, 2^20 = 1M paths would be explored.
        // The call counter (limit 256) ensures this returns quickly.
        let rule = parse_bash_rule("Bash(cmd nonexistent *)").unwrap();
        let flags: Vec<String> = (0..20).map(|i| format!("--f{i} v{i}")).collect();
        let cmd = format!("cmd {} other", flags.join(" "));
        assert!(!matches_flexible(&rule, &cmd));
    }
}
