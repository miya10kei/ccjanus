mod cli;
mod config;
mod judge;
mod output;
mod parser;
mod permission;
mod types;

use std::io::Read;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};
use config::{discover_settings_files, load_permission_set};
use judge::judge;
use output::{emit_allow, emit_deny, emit_fallthrough};
use parser::parse_command;
use permission::parse_bash_rule;
use types::{HookInput, Judgment, PermissionSet};

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        None => run_hook_mode(cli.debug, cli.explain),
        Some(Command::Doctor) => run_doctor_mode(),
        Some(Command::Parse) => run_parse_mode(),
        Some(Command::Simulate {
            command,
            permissions,
            deny,
        }) => run_simulate_mode(command, permissions, deny, cli.debug, cli.explain),
    };

    if let Err(e) = result {
        if cli.debug {
            eprintln!("[ccjanus] error: {e}");
        }
        // Fallthrough on error
    }
}

fn run_hook_mode(debug: bool, explain: bool) -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            if debug {
                eprintln!("[ccjanus] JSON parse error: {e}");
            }
            return Ok(());
        }
    };

    // Only handle Bash tool
    match hook_input.tool_name.as_deref() {
        Some("Bash") => {}
        _ => return Ok(()),
    }

    let command = match hook_input.tool_input.and_then(|ti| ti.command) {
        Some(cmd) => cmd,
        None => return Ok(()),
    };

    let permissions = load_permission_set(debug)?;

    match judge(&command, &permissions, debug, explain) {
        Judgment::Allow => emit_allow(),
        Judgment::Deny(reason) => emit_deny(&reason),
        Judgment::Fallthrough(reason) => emit_fallthrough(&reason, explain),
    }

    Ok(())
}

fn run_doctor_mode() -> Result<()> {
    let files = discover_settings_files();
    let permissions = load_permission_set(false)?;

    println!("Settings files:");
    for file in &files {
        let status = if file.exists { "found" } else { "not found" };
        println!("  [{status}] {} ({})", file.path.display(), file.source);
    }

    println!("\nAllow rules ({}):", permissions.allow.len());
    for rule in &permissions.allow {
        let wildcard = rule.segments.len() > 1;
        let pattern = rule.segments.join("*");
        println!(
            "  {} -> pattern: '{}', wildcard: {}",
            rule.original, pattern, wildcard
        );
    }

    println!("\nDeny rules ({}):", permissions.deny.len());
    for rule in &permissions.deny {
        let wildcard = rule.segments.len() > 1;
        let pattern = rule.segments.join("*");
        println!(
            "  {} -> pattern: '{}', wildcard: {}",
            rule.original, pattern, wildcard
        );
    }

    Ok(())
}

fn run_parse_mode() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let command = input.trim();
    if command.is_empty() {
        println!("No command provided");
        return Ok(());
    }

    let segments = parse_command(command)?;

    println!("Command: {command}");
    println!("Segments ({}):", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        println!(
            "  [{i}] name: '{}', full: '{}'",
            seg.command_name, seg.full_text
        );
    }

    Ok(())
}

fn run_simulate_mode(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
    debug: bool,
    explain: bool,
) -> Result<()> {
    let permissions = PermissionSet {
        allow: allow_rules
            .iter()
            .filter_map(|s| parse_bash_rule(s))
            .collect(),
        deny: deny_rules
            .iter()
            .filter_map(|s| parse_bash_rule(s))
            .collect(),
    };

    let result = judge(command, &permissions, debug, explain);

    match &result {
        Judgment::Allow => println!("Result: ALLOW"),
        Judgment::Deny(reason) => println!("Result: DENY ({reason})"),
        Judgment::Fallthrough(reason) => println!("Result: FALLTHROUGH ({reason})"),
    }

    Ok(())
}
