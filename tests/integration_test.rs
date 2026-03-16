use std::io::Write;
use std::process::{Command, Stdio};

fn ccjanus_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ccjanus"))
}

#[test]
fn test_hook_mode_allow_simple() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"permissions":{"allow":["Bash(ls *)"]}}"#,
    )
    .unwrap();

    let mut child = ccjanus_bin()
        .env("CLAUDE_CONFIG_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"ls -la"}}"#)
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("approve"));
}

#[test]
fn test_hook_mode_fallthrough_non_bash() {
    let mut child = ccjanus_bin()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(br#"{"tool_name":"Read","tool_input":{"file":"/tmp/test"}}"#)
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.is_empty());
}

#[test]
fn test_hook_mode_compound_all_allowed() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"permissions":{"allow":["Bash(ls *)","Bash(grep *)"]}}"#,
    )
    .unwrap();

    let mut child = ccjanus_bin()
        .env("CLAUDE_CONFIG_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"ls /tmp | grep test"}}"#)
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("approve"));
}

#[test]
fn test_hook_mode_compound_partial() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"permissions":{"allow":["Bash(ls *)"]}}"#,
    )
    .unwrap();

    let mut child = ccjanus_bin()
        .env("CLAUDE_CONFIG_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"ls /tmp | grep test"}}"#)
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should fallthrough (no output)
    assert!(stdout.is_empty());
}

#[test]
fn test_parse_mode() {
    let mut child = ccjanus_bin()
        .arg("parse")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(b"ls /tmp | grep test").unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ls"));
    assert!(stdout.contains("grep"));
    assert!(stdout.contains("Segments (2)"));
}

#[test]
fn test_simulate_mode_allow() {
    let output = ccjanus_bin()
        .args([
            "simulate",
            "--command",
            "ls -la",
            "--permissions",
            "Bash(ls *)",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ALLOW"));
}

#[test]
fn test_simulate_mode_compound_allow() {
    let output = ccjanus_bin()
        .args([
            "simulate",
            "--command",
            "ls | grep foo",
            "--permissions",
            "Bash(ls *)",
            "--permissions",
            "Bash(grep *)",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ALLOW"));
}

#[test]
fn test_simulate_mode_fallthrough() {
    let output = ccjanus_bin()
        .args([
            "simulate",
            "--command",
            "rm -rf /",
            "--permissions",
            "Bash(ls *)",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("FALLTHROUGH"));
}

#[test]
fn test_hook_mode_invalid_json_fallthrough() {
    let mut child = ccjanus_bin()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(b"not json").unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.is_empty());
}

#[test]
fn test_hook_mode_deny_compound() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"permissions":{"allow":["Bash(ls *)","Bash(rm *)"],"deny":["Bash(rm *)"]}}"#,
    )
    .unwrap();

    let mut child = ccjanus_bin()
        .env("CLAUDE_CONFIG_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"ls /tmp | rm -rf /"}}"#)
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    // Deny exits with code 2
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn test_hook_mode_deny_interior_wildcard() {
    let tmp = tempfile::TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"permissions":{"allow":["Bash(git *)"],"deny":["Bash(git push * main)"]}}"#,
    )
    .unwrap();

    let mut child = ccjanus_bin()
        .env("CLAUDE_CONFIG_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"git push origin main"}}"#)
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Simple command matching deny -> fallthrough (no output)
    assert!(stdout.is_empty());
}

#[test]
fn test_simulate_mode_interior_wildcard_deny() {
    let output = ccjanus_bin()
        .args([
            "simulate",
            "--command",
            "git push origin main",
            "--permissions",
            "Bash(git *)",
            "--deny",
            "Bash(git push * main)",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("FALLTHROUGH"));
}

#[test]
fn test_simulate_mode_interior_wildcard_no_deny() {
    let output = ccjanus_bin()
        .args([
            "simulate",
            "--command",
            "git push origin develop",
            "--permissions",
            "Bash(git *)",
            "--deny",
            "Bash(git push * main)",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ALLOW"));
}
