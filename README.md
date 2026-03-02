# sort-gitignore

Zed extension that sorts `.gitignore` entries alphabetically within sections.

## Required Layout

- Keep grammar artifacts in `grammars/` (for example `grammars/gitignore.wasm`).
- Do not keep duplicate grammar repos in `vendor/`.
- The language server source belongs in `git_ignore-lsp/`.

## Prerequisites

- Rust toolchain with `cargo`
- Wasm target:

```bash
rustup target add wasm32-wasip1
```

## Build

1. Build and test language server:

```bash
cargo test -p git_ignore-lsp
```

2. Build extension WASM (this file must exist as `extension.wasm`):

```bash
cargo build --release --target wasm32-wasip1
cp target/wasm32-wasip1/release/sort_git_ignore_extension.wasm extension.wasm
```

3. Optional local LSP binary for dev use (fallback when GitHub release is unavailable):

```bash
cargo build -p git_ignore-lsp --release
ln -sf "$PWD/target/release/git_ignore-lsp" "$HOME/.local/bin/git_ignore-lsp"
```

## Install Dev Extension In Zed

1. Run `Extensions: Install Dev Extension`.
2. Select this repo folder.
3. Ensure `extension.wasm` exists in repo root before installing.

If the extension still does not appear or updates are stale, clear installed
cache and reinstall:

```bash
rm -rf ~/.local/share/zed/extensions/installed/sort-git_ignore
```

## Zed Settings

```jsonc
"languages": {
  "Git Ignore": {
    "format_on_save": "on",
    "formatter": {
      "language_server": {
        "name": "gitignore-lsp"
      }
    }
  }
},
"lsp": {
  "gitignore-lsp": {
    "enable_lsp_tasks": true
  }
}
```

## Troubleshooting

### `could not find 'git_ignore-lsp' on PATH`

The extension first tries to download a pre-built binary from the latest GitHub
release. If that fetch fails (e.g. no network access, rate-limited, or no
matching asset for your platform), it falls back to looking for `git_ignore-lsp`
on your `PATH`.

To satisfy the fallback, build and symlink the binary locally:

```bash
cargo build -p git_ignore-lsp --release
ln -sf "$PWD/target/release/git_ignore-lsp" "$HOME/.local/bin/git_ignore-lsp"
```

Make sure `$HOME/.local/bin` (or whichever directory you link into) is present
in your `PATH`. You can verify with:

```bash
which git_ignore-lsp
```

Then reload the extension in Zed (`Extensions: Reload Extension`) and the
language server should start successfully.

## Verify It Works

Create a `.gitignore` file with unsorted entries and save.
It should be rewritten in sorted order within each section.
