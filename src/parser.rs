use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::types::CommandSegment;

const BASH_C_RECURSION_LIMIT: usize = 10;

/// Check if a command is a simple (non-compound) command.
pub fn is_simple_command(command: &str) -> bool {
    let compound_indicators = ["|", "&", ";", "$(", "<(", ">(", "`"];
    for indicator in &compound_indicators {
        if command.contains(indicator) {
            return false;
        }
    }

    let lower = command.to_lowercase();
    if lower.starts_with("bash -c") || lower.starts_with("sh -c") {
        return false;
    }

    true
}

/// Parse a command string into segments using tree-sitter-bash.
pub fn parse_command(command: &str) -> Result<Vec<CommandSegment>> {
    let mut parser = Parser::new();
    let language = tree_sitter_bash::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| anyhow::anyhow!("Failed to set language: {e}"))?;

    let tree = parser
        .parse(command, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse command"))?;

    let root = tree.root_node();
    let source = command.as_bytes();

    let mut segments = Vec::new();
    collect_segments(root, source, &mut segments, 0);

    Ok(segments)
}

/// Extract the command name from a command string, stripping env var prefixes.
pub fn extract_command_name(command: &str) -> String {
    let trimmed = command.trim();

    // Strip environment variable prefixes (e.g., "FOO=bar cmd" -> "cmd")
    let mut rest = trimmed;
    loop {
        let word = match rest.split_whitespace().next() {
            Some(w) => w,
            None => return String::new(),
        };

        if word.contains('=') && !word.starts_with('=') {
            rest = rest[word.len()..].trim_start();
        } else {
            return word.to_string();
        }
    }
}

fn collect_segments(node: Node, source: &[u8], segments: &mut Vec<CommandSegment>, depth: usize) {
    if depth > BASH_C_RECURSION_LIMIT {
        return;
    }

    match node.kind() {
        "command" => {
            let full_text = node_text(node, source);
            let cmd_name = extract_command_name(&full_text);

            // Handle bash -c / sh -c
            if (cmd_name == "bash" || cmd_name == "sh") && is_dash_c_command(&full_text) {
                if let Some(inner) = extract_bash_c_inner(&full_text) {
                    if let Ok(inner_segments) = parse_command_with_depth(&inner, depth + 1) {
                        segments.extend(inner_segments);
                        return;
                    }
                }
            }

            segments.push(CommandSegment {
                command_name: cmd_name,
                full_text,
            });
        }
        "redirected_statement" => {
            // Process only the command part, ignoring redirections
            for i in 0..node.child_count() {
                let child = node.child(i).unwrap();
                if child.kind() != "file_redirect"
                    && child.kind() != "heredoc_redirect"
                    && child.kind() != "herestring_redirect"
                {
                    collect_segments(child, source, segments, depth);
                }
            }
        }
        "pipeline"
        | "list"
        | "subshell"
        | "command_substitution"
        | "process_substitution"
        | "compound_statement"
        | "if_statement"
        | "for_statement"
        | "while_statement"
        | "case_statement"
        | "program" => {
            for i in 0..node.child_count() {
                let child = node.child(i).unwrap();
                collect_segments(child, source, segments, depth);
            }
        }
        _ => {
            // Recurse into unknown node types to find commands
            for i in 0..node.child_count() {
                let child = node.child(i).unwrap();
                collect_segments(child, source, segments, depth);
            }
        }
    }
}

fn parse_command_with_depth(command: &str, depth: usize) -> Result<Vec<CommandSegment>> {
    if depth > BASH_C_RECURSION_LIMIT {
        anyhow::bail!("Recursion limit reached");
    }

    let mut parser = Parser::new();
    let language = tree_sitter_bash::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| anyhow::anyhow!("Failed to set language: {e}"))?;

    let tree = parser
        .parse(command, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse command"))?;

    let root = tree.root_node();
    let source = command.as_bytes();

    let mut segments = Vec::new();
    collect_segments(root, source, &mut segments, depth);

    Ok(segments)
}

fn node_text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

fn is_dash_c_command(command: &str) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    parts.len() >= 3 && parts[1] == "-c"
}

fn extract_bash_c_inner(command: &str) -> Option<String> {
    // Find the position after "-c"
    let parts: Vec<&str> = command.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return None;
    }

    let inner = parts[2..].join(" ");
    // Strip surrounding quotes
    let trimmed = inner.trim();
    if (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('"') && trimmed.ends_with('"'))
    {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_simple_command() {
        assert!(is_simple_command("ls -la"));
        assert!(is_simple_command("cat file.txt"));
        assert!(!is_simple_command("ls | grep foo"));
        assert!(!is_simple_command("ls && echo done"));
        assert!(!is_simple_command("echo $(date)"));
        assert!(!is_simple_command("bash -c 'ls'"));
        assert!(!is_simple_command("sh -c 'ls'"));
    }

    #[test]
    fn test_parse_simple_command() {
        let segments = parse_command("ls -la").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].command_name, "ls");
    }

    #[test]
    fn test_parse_pipeline() {
        let segments = parse_command("ls /tmp | grep test").unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].command_name, "ls");
        assert_eq!(segments[1].command_name, "grep");
    }

    #[test]
    fn test_parse_list() {
        let segments = parse_command("ls && echo done").unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].command_name, "ls");
        assert_eq!(segments[1].command_name, "echo");
    }

    #[test]
    fn test_parse_semicolon() {
        let segments = parse_command("cd /tmp; ls").unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].command_name, "cd");
        assert_eq!(segments[1].command_name, "ls");
    }

    #[test]
    fn test_extract_command_name_with_env() {
        assert_eq!(extract_command_name("FOO=bar ls -la"), "ls");
        assert_eq!(extract_command_name("A=1 B=2 cmd arg"), "cmd");
        assert_eq!(extract_command_name("ls"), "ls");
    }

    #[test]
    fn test_parse_bash_c() {
        let segments = parse_command("bash -c 'ls -la'").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].command_name, "ls");
    }

    #[test]
    fn test_parse_complex_pipeline() {
        let segments = parse_command("cat file | sort | uniq -c | head -5").unwrap();
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].command_name, "cat");
        assert_eq!(segments[1].command_name, "sort");
        assert_eq!(segments[2].command_name, "uniq");
        assert_eq!(segments[3].command_name, "head");
    }

    #[test]
    fn test_parse_with_redirect() {
        let segments = parse_command("echo hello > /tmp/out").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].command_name, "echo");
    }
}
