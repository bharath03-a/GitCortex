# Dev Setup

This is the long-form bootstrap guide for setting up a fresh machine. If you just want the three-line version, see `CONTRIBUTING.md`.

## Required tools

| Tool | Version | Purpose |
|---|---|---|
| Git | 2.30+ | obviously |
| Rust | 1.95+ | the workspace pins MSRV at 1.95 in `rust-toolchain.toml` |
| Node | 20+ | builds the React viz frontend |
| C compiler | system | KuzuDB has C++ static libs that need a working `cc` |

### Recommended (one-shot setup)

The project ships a `mise.toml` that pins both Rust and Node versions. Install [mise](https://mise.jdx.dev) once, and you're done:

```bash
brew install mise          # macOS
# or: curl https://mise.run | sh
mise install               # reads mise.toml → installs rust 1.95 + node 20
```

We also ship a `Justfile`. Install [just](https://github.com/casey/just):

```bash
brew install just          # macOS
# or: cargo install just
```

With both installed:

```bash
just bootstrap             # runs: cargo fetch + cd viz && npm ci
just dev                   # backend (Axum on :5678) + viz HMR (Vite on :5173) in parallel
just ci                    # mirrors GitHub Actions locally
```

## Manual setup (if you can't / won't install mise + just)

```bash
# 1. Rust
rustup install 1.95
rustup component add rustfmt clippy

# 2. Node (use your preferred version manager — fnm, nvm, volta, asdf...)
node --version             # expect v20.x

# 3. Fetch deps
cargo fetch
cd crates/gitcortex-mcp/viz && npm ci && cd -

# 4. Dev loop (two terminals)
# Terminal 1
cargo run -p gitcortex -- viz --port 5678
# Terminal 2
cd crates/gitcortex-mcp/viz && npm run dev
```

## Platform-specific notes

### macOS

KuzuDB's C++ static libs require `MACOSX_DEPLOYMENT_TARGET` to match your SDK. We set this in `.cargo/config.toml`. If you get linker errors about missing kuzu symbols on macOS 14+, check that file.

### Linux

`apt install build-essential cmake` covers everything KuzuDB needs.

### Windows

Cross-platform builds are tested in CI but the primary dev environment is Unix-like. If you hit issues on Windows, please open an issue with the full error.

## Editor setup

### VS Code / Cursor

The repo ships `.vscode/` with recommended extensions. You'll be prompted on first open.

Recommended extensions:
- `rust-lang.rust-analyzer`
- `tamasfe.even-better-toml`
- `bradlc.vscode-tailwindcss`
- `dbaeumer.vscode-eslint`
- `esbenp.prettier-vscode`

### IntelliJ / RustRover

Open the repo as a Cargo project. RustRover handles the workspace automatically.

### Vim / Neovim / Emacs

`rust-analyzer` is the LSP. Standard `tsserver` or `vtsls` for the viz frontend.

## Common pitfalls

### "Can't set lock on file /Users/.../graph.kuzu"

Two `gcx` processes are competing for the same KuzuDB. Kill any background `gcx viz` (`pkill -f "gcx viz"`) before running another command that touches the store.

### "Viz frontend not built"

The Rust binary serves an HTML stub saying "viz frontend not built". You need to run `npm run build` in `viz/` to produce the embedded bundle, or rebuild with the auto-build `build.rs` (`cargo build` should do this).

### Pre-commit hook prints "no changes"

That's intentional. `gcx hook` is idempotent — if `last_indexed_sha == HEAD`, it exits without doing anything.

### Tests are slow

Add `--features memory` once Phase H lands to skip the KuzuDB C++ link in unit tests.

## Optional: reproducible env via Nix

If you prefer a fully pinned environment, the repo includes a `flake.nix`:

```bash
nix develop                # drops you in a shell with rust + node + cmake + git
```

This is overkill for most contributors but useful for CI debugging and air-gapped dev environments.

## Where to read next

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — how the crates fit together
- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — per-track contributor guide
- [`adr/`](adr/) — design decisions
