use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub tool_name: Option<String>,
    pub tool_input: Option<ToolInput>,
}

#[derive(Debug, Deserialize)]
pub struct ToolInput {
    pub command: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PermissionRule {
    pub original: String,
    pub segments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandSegment {
    pub command_name: String,
    pub full_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Judgment {
    Allow,
    Deny(String),
    Fallthrough(String),
}

#[derive(Clone, Debug, Default)]
pub struct PermissionSet {
    pub allow: Vec<PermissionRule>,
    pub deny: Vec<PermissionRule>,
}

#[derive(Debug, Serialize)]
pub struct HookOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SettingsFileInfo {
    pub path: std::path::PathBuf,
    pub exists: bool,
    pub source: SettingsSource,
}

#[derive(Clone, Debug)]
pub enum SettingsSource {
    GlobalConfig,
    GlobalLocal,
    ProjectConfig,
    ProjectLocal,
}

impl std::fmt::Display for SettingsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsSource::GlobalConfig => write!(f, "global config"),
            SettingsSource::GlobalLocal => write!(f, "global local"),
            SettingsSource::ProjectConfig => write!(f, "project config"),
            SettingsSource::ProjectLocal => write!(f, "project local"),
        }
    }
}
