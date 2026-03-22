use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ccjanus",
    version,
    about = "Claude Code PreToolUse hook for auto-approving bash commands"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Enable debug output to stderr
    #[arg(long, global = true)]
    pub debug: bool,

    /// Show explanation of the judgment
    #[arg(long, global = true)]
    pub explain: bool,

    /// Enable flexible matching that strips option arguments before matching
    #[arg(long, global = true)]
    pub flexible_match: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Parse a command from stdin and show segments
    Parse,

    /// Simulate a judgment for a command
    Simulate {
        /// The command to simulate
        #[arg(long)]
        command: String,

        /// Permission rules (e.g., 'Bash(ls *)')
        #[arg(long = "permissions")]
        permissions: Vec<String>,

        /// Deny rules (e.g., 'Bash(rm *)')
        #[arg(long)]
        deny: Vec<String>,
    },

    /// Show settings file status
    Doctor,
}
