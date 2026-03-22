use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;

use crate::permission::parse_bash_rule;
use crate::types::{PermissionSet, SettingsFileInfo, SettingsSource};

/// Load and merge permission rules from all settings files.
pub fn load_permission_set(debug: bool) -> Result<PermissionSet> {
    let files = discover_settings_files();
    let mut perm_set = PermissionSet::default();

    for file_info in &files {
        if !file_info.exists {
            continue;
        }

        match load_permissions_from_file(&file_info.path) {
            Ok((allow, deny, flexible_match)) => {
                perm_set.allow.extend(allow);
                perm_set.deny.extend(deny);
                if flexible_match {
                    perm_set.flexible_match = true;
                }
            }
            Err(e) => {
                if debug {
                    eprintln!(
                        "[ccjanus] warning: failed to parse {}: {e}",
                        file_info.path.display()
                    );
                }
            }
        }
    }

    Ok(perm_set)
}

/// Discover all settings files in order.
pub fn discover_settings_files() -> Vec<SettingsFileInfo> {
    let mut files = Vec::new();

    let config_dir = claude_config_dir();

    // Global config
    let global_config = config_dir.join("settings.json");
    files.push(SettingsFileInfo {
        exists: global_config.exists(),
        path: global_config,
        source: SettingsSource::GlobalConfig,
    });

    // Global local
    let global_local = config_dir.join("settings.local.json");
    files.push(SettingsFileInfo {
        exists: global_local.exists(),
        path: global_local,
        source: SettingsSource::GlobalLocal,
    });

    // Project config
    if let Some(git_root) = git_root_dir() {
        let project_config = git_root.join(".claude").join("settings.json");
        files.push(SettingsFileInfo {
            exists: project_config.exists(),
            path: project_config,
            source: SettingsSource::ProjectConfig,
        });

        let project_local = git_root.join(".claude").join("settings.local.json");
        files.push(SettingsFileInfo {
            exists: project_local.exists(),
            path: project_local,
            source: SettingsSource::ProjectLocal,
        });
    }

    files
}

fn claude_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
}

fn git_root_dir() -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    Some(PathBuf::from(path.trim()))
}

fn load_permissions_from_file(
    path: &std::path::Path,
) -> Result<(
    Vec<crate::types::PermissionRule>,
    Vec<crate::types::PermissionRule>,
    bool,
)> {
    let content = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut allow_rules = Vec::new();
    let mut deny_rules = Vec::new();

    // Parse "permissions" -> "allow" array
    if let Some(allow_arr) = value
        .pointer("/permissions/allow")
        .and_then(|v| v.as_array())
    {
        for item in allow_arr {
            if let Some(s) = item.as_str() {
                if let Some(rule) = parse_bash_rule(s) {
                    allow_rules.push(rule);
                }
            }
        }
    }

    // Parse "permissions" -> "deny" array
    if let Some(deny_arr) = value
        .pointer("/permissions/deny")
        .and_then(|v| v.as_array())
    {
        for item in deny_arr {
            if let Some(s) = item.as_str() {
                if let Some(rule) = parse_bash_rule(s) {
                    deny_rules.push(rule);
                }
            }
        }
    }

    // Parse "permissions" -> "flexible_match" boolean
    let flexible_match = value
        .pointer("/permissions/flexible_match")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok((allow_rules, deny_rules, flexible_match))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_settings_file(dir: &std::path::Path, content: &str) -> PathBuf {
        let path = dir.join("settings.json");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_load_permissions_from_file() {
        let tmp = TempDir::new().unwrap();
        let path = create_settings_file(
            tmp.path(),
            r#"{
                "permissions": {
                    "allow": ["Bash(ls *)", "Bash(cat *)"],
                    "deny": ["Bash(rm *)"]
                }
            }"#,
        );

        let (allow, deny, flexible) = load_permissions_from_file(&path).unwrap();
        assert_eq!(allow.len(), 2);
        assert_eq!(deny.len(), 1);
        assert_eq!(allow[0].segments, vec!["ls ", ""]);
        assert_eq!(allow[1].segments, vec!["cat ", ""]);
        assert_eq!(deny[0].segments, vec!["rm ", ""]);
        assert!(!flexible);
    }

    #[test]
    fn test_load_permissions_non_bash_ignored() {
        let tmp = TempDir::new().unwrap();
        let path = create_settings_file(
            tmp.path(),
            r#"{
                "permissions": {
                    "allow": ["Bash(ls *)", "Read(*)", "Edit(*)"]
                }
            }"#,
        );

        let (allow, _deny, _flexible) = load_permissions_from_file(&path).unwrap();
        assert_eq!(allow.len(), 1);
        assert_eq!(allow[0].segments, vec!["ls ", ""]);
    }

    #[test]
    fn test_load_permissions_missing_permissions_key() {
        let tmp = TempDir::new().unwrap();
        let path = create_settings_file(tmp.path(), r#"{"other": "value"}"#);

        let (allow, deny, flexible) = load_permissions_from_file(&path).unwrap();
        assert!(allow.is_empty());
        assert!(deny.is_empty());
        assert!(!flexible);
    }

    #[test]
    fn test_load_permissions_flexible_match_true() {
        let tmp = TempDir::new().unwrap();
        let path = create_settings_file(
            tmp.path(),
            r#"{
                "permissions": {
                    "allow": ["Bash(ls *)"],
                    "flexible_match": true
                }
            }"#,
        );

        let (_allow, _deny, flexible) = load_permissions_from_file(&path).unwrap();
        assert!(flexible);
    }

    #[test]
    fn test_load_permissions_flexible_match_false() {
        let tmp = TempDir::new().unwrap();
        let path = create_settings_file(
            tmp.path(),
            r#"{
                "permissions": {
                    "allow": ["Bash(ls *)"],
                    "flexible_match": false
                }
            }"#,
        );

        let (_allow, _deny, flexible) = load_permissions_from_file(&path).unwrap();
        assert!(!flexible);
    }

    #[test]
    fn test_load_permissions_flexible_match_missing() {
        let tmp = TempDir::new().unwrap();
        let path = create_settings_file(
            tmp.path(),
            r#"{
                "permissions": {
                    "allow": ["Bash(ls *)"]
                }
            }"#,
        );

        let (_allow, _deny, flexible) = load_permissions_from_file(&path).unwrap();
        assert!(!flexible);
    }

    #[test]
    fn test_load_permission_set_with_env() {
        let tmp = TempDir::new().unwrap();
        create_settings_file(
            tmp.path(),
            r#"{
                "permissions": {
                    "allow": ["Bash(ls *)"]
                }
            }"#,
        );

        std::env::set_var("CLAUDE_CONFIG_DIR", tmp.path());
        let perm_set = load_permission_set(false).unwrap();
        std::env::remove_var("CLAUDE_CONFIG_DIR");

        assert!(!perm_set.allow.is_empty());
        assert_eq!(perm_set.allow[0].segments, vec!["ls ", ""]);
    }
}
