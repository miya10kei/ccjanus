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

## 柔軟マッチング

コマンドに `--group dev` や `-v` のようなオプション引数が含まれている場合、パーミッションルールにマッチしないことがあります。例えば、`uv run --group dev ruff format file.py` は `Bash(uv run ruff format *)` にマッチしません。オプション引数がプレフィックスの順序マッチングを妨げるためです。

**柔軟マッチング**を有効にすると、マッチング前にオプション引数を自動的にストリップします：

### settings.json で設定する場合

```json
{
  "permissions": {
    "allow": ["Bash(uv run ruff format *)"],
    "flexible_match": true
  }
}
```

### CLIフラグで指定する場合

```bash
ccjanus --flexible-match
ccjanus simulate --flexible-match --command 'uv run --group dev ruff format file.py' --permissions 'Bash(uv run ruff format *)'
```

有効にすると、ccjanusは通常のマッチングが失敗した場合に、コマンドから `-x val`、`--flag val`、`--flag=val` パターンをストリップしてから再度マッチングを試みます。通常の（完全な）マッチングは常に最初に試行されます。

**注意:** 複数の設定ファイルが読み込まれる場合、いずれかのファイルで `flexible_match: true` が設定されていると、この機能がグローバルに有効になります。プロジェクトレベルで `false` を設定しても、グローバルの `true` を上書きすることはできません。

**制約:** ルールにフラグ風のトークンが含まれている場合（例: `Bash(python -m pytest *)`、`Bash(docker run * --rm)`）、それらは通常の（完全な）マッチングでマッチされます。柔軟マッチングはコマンドからフラグをストリップするため、ルール内のフラグパターンはストリップ経由ではマッチできません。これは自動的に処理されます — 通常のマッチングが常に最初に試行されます。

## PATH対応マッチング

絶対パスでコマンドが呼び出された場合（例: `/usr/bin/grep foo`）、bare な名前で書かれたパーミッションルール（例: `Bash(grep *)`）にはマッチしません。マッチングは純粋な文字列ベースのためです。ユーザーがすべての絶対パスを列挙しなくて済むよう、ccjanus はコマンド先頭の絶対パスを basename に正規化します（その親ディレクトリが `$PATH` に含まれている場合のみ）。

**デフォルトで有効です。** allow と deny の両方に適用されるため、フラグや絶対パスを追加しても deny ルールをバイパスできません。

### 動作

- 各コマンドセグメント（パイプや `&&` 等で分割した個別コマンド）の **先頭トークンのみ** を対象とします。
- 以下を **すべて満たす** 場合のみ正規化されます:
  1. `/` で始まる絶対パスである
  2. その親ディレクトリが `$PATH` のいずれかのエントリと完全一致する
- 満たさない場合は元のコマンドのまま使用されます。

例（`PATH=/usr/bin` の場合）:

| コマンド | ルール | マッチ? |
|---------|--------|--------|
| `/usr/bin/grep foo` | `Bash(grep *)` | ✅（`grep foo` に正規化） |
| `/tmp/evil/grep foo` | `Bash(grep *)` | ❌（`/tmp/evil` は `$PATH` にない） |
| `./grep foo` | `Bash(grep *)` | ❌（絶対パスでない） |
| `/bin/rm /tmp/x` | deny `Bash(rm *)` | ✅ deny にマッチ（正規化される） |

### 無効化

settings.json での制御は **ありません**。CLI フラグを使用してください:

```bash
ccjanus --no-path-normalize
ccjanus simulate --no-path-normalize --command '/usr/bin/grep foo' --permissions 'Bash(grep *)'
```

### 制約

- 正規化は `$PATH` のエントリと文字列を厳密に比較します。末尾スラッシュやシンボリックリンクは正規化されません。
- 環境変数プリフィクス（例: `FOO=bar /usr/bin/ls`）は複合コマンドのセグメントでは `command_name` が抽出されるため動作しますが、純粋な単純コマンドではプリフィクス付きのまま扱われます。

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
