# ccjanus

Claude Code PreToolUse Hook for auto-approving bash commands. When piped/compound bash commands consist entirely of individually permitted commands, ccjanus automatically approves them.

## Installation

### mise (recommended)

```bash
mise use ubi:miya10kei/ccjanus
```

### Homebrew

```bash
brew install miya10kei/ccjanus/ccjanus
```

### Build from source

```bash
cargo build --release
cp target/release/ccjanus ~/.local/bin/
```

## Hook Configuration

Add to your Claude Code settings (`~/.claude/settings.json`):

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "ccjanus"
          }
        ]
      }
    ]
  },
  "permissions": {
    "allow": [
      "Bash(ls *)",
      "Bash(cat *)",
      "Bash(grep *)",
      "Bash(head *)",
      "Bash(tail *)",
      "Bash(wc *)",
      "Bash(sort *)",
      "Bash(uniq *)"
    ],
    "deny": [
      "Bash(rm *)"
    ]
  }
}
```

## How It Works

1. Reads hook input (JSON) from stdin
2. Parses the bash command using tree-sitter-bash
3. Checks each command segment against allow/deny rules from settings files
4. Outputs a judgment: approve, block, or fallthrough (let Claude Code handle it)

**Simple commands**: deny match -> fallthrough, allow match -> approve, no match -> fallthrough

**Compound commands** (pipes, `&&`, `;`, etc.): any deny match -> block (exit 2), all segments allowed -> approve, otherwise -> fallthrough

## Settings File Lookup Order

1. `$CLAUDE_CONFIG_DIR/settings.json` (fallback: `~/.claude/settings.json`)
2. `$CLAUDE_CONFIG_DIR/settings.local.json`
3. `<git-root>/.claude/settings.json`
4. `<git-root>/.claude/settings.local.json`

## Permission Rule Formats

| Format | Meaning |
|--------|---------|
| `Bash(ls *)` | Allow any `ls` command |
| `Bash(ls:*)` | Same as above (colon separator) |
| `Bash(ls)` | Allow only bare `ls` (no arguments) |
| `Bash(*)` | Allow everything |

## CLI Modes

### Hook Mode (default)

```bash
echo '{"tool_name":"Bash","tool_input":{"command":"ls /tmp | grep test"}}' | ccjanus --explain
```

### Parse Mode

```bash
echo 'ls /tmp | grep test' | ccjanus parse
```

### Simulate Mode

```bash
ccjanus simulate --command 'ls | grep foo' --permissions 'Bash(ls *)' --permissions 'Bash(grep *)'
```

### Doctor Mode

```bash
ccjanus doctor
```

## Error Handling

When in doubt, ccjanus falls through. It never blocks Claude Code due to its own errors.

## License

MIT
