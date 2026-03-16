# ccjanus

> [English version](README.md)

Claude Code の PreToolUse Hook として動作し、bashコマンドを自動承認するツールです。パイプや複合コマンドが、個別に許可されたコマンドのみで構成されている場合、ccjanusは自動的にそれらを承認します。

## インストール

### mise（推奨）

```bash
mise use ubi:miya10kei/ccjanus
```

### ソースからビルド

```bash
cargo build --release
cp target/release/ccjanus ~/.local/bin/
```

## Hook設定

Claude Codeの設定ファイル（`~/.claude/settings.json`）に以下を追加してください：

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

## 仕組み

1. stdinからHook入力（JSON）を読み取る
2. tree-sitter-bashを使用してbashコマンドを解析する
3. 設定ファイルのallow/denyルールに対して各コマンドセグメントをチェックする
4. 判定結果を出力する：承認（approve）、ブロック（block）、またはフォールスルー（fallthrough、Claude Codeに判断を委ねる）

**単純コマンドの場合**：denyに一致 → フォールスルー、allowに一致 → 承認、一致なし → フォールスルー

**複合コマンドの場合**（パイプ、`&&`、`;` など）：いずれかがdenyに一致 → ブロック（exit 2）、全セグメントがallowに一致 → 承認、それ以外 → フォールスルー

## 設定ファイルの読み込み順序

1. `$CLAUDE_CONFIG_DIR/settings.json`（フォールバック：`~/.claude/settings.json`）
2. `$CLAUDE_CONFIG_DIR/settings.local.json`
3. `<git-root>/.claude/settings.json`
4. `<git-root>/.claude/settings.local.json`

## パーミッションルールの書式

| 書式 | 意味 |
|------|------|
| `Bash(ls *)` | すべての`ls`コマンドを許可 |
| `Bash(ls:*)` | 上記と同じ（コロン区切り） |
| `Bash(ls)` | 引数なしの`ls`のみ許可 |
| `Bash(*)` | すべてを許可 |

## CLIモード

### Hookモード（デフォルト）

```bash
echo '{"tool_name":"Bash","tool_input":{"command":"ls /tmp | grep test"}}' | ccjanus --explain
```

### Parseモード

```bash
echo 'ls /tmp | grep test' | ccjanus parse
```

### Simulateモード

```bash
ccjanus simulate --command 'ls | grep foo' --permissions 'Bash(ls *)' --permissions 'Bash(grep *)'
```

### Doctorモード

```bash
ccjanus doctor
```

## エラーハンドリング

判断に迷った場合、ccjanusはフォールスルーします。ccjanus自体のエラーによってClaude Codeをブロックすることはありません。

## ライセンス

MIT
