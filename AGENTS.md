# Repository Guidelines

## Project Structure & Module Organization

Lapce is a Cargo workspace containing four crates:

- `lapce-app/`: Floem-based editor UI, application state, commands, and the `lapce` binary.
- `lapce-core/`: editor primitives such as ropes, syntax, language metadata, and text positions.
- `lapce-proxy/`: local/remote proxy services, terminals, plugins, and the `lapce-proxy` binary.
- `lapce-rpc/`: shared RPC and protocol data types.

User defaults and schemas live in `defaults/` and `extra/schemas/`; documentation is in `docs/`; packaging assets and platform resources are under `extra/` and `icons/`. Unit tests normally sit beside the implementation in `src/` modules. The main benchmark is `lapce-app/benches/visual_line.rs`.

## Build, Test, and Development Commands

Run commands from the repository root:

```sh
cargo fetch --locked                 # Fetch the pinned dependency graph
cargo build --frozen --bin lapce      # Build the main application
cargo run --profile fastdev --bin lapce  # Run a faster local development build
cargo test --workspace               # Run all unit and integration tests
cargo test --doc --workspace         # Run documentation tests (matches CI)
cargo fmt --all --check              # Verify formatting
cargo clippy                         # Run Clippy checks
cargo bench -p lapce-app             # Run Criterion benchmarks
```

The Rust toolchain is managed by `rust-toolchain.toml` (workspace minimum Rust 1.87). Local development uses Cargo; the automated release workflow currently targets Windows.

## Coding Style & Naming Conventions

Use Rust 2024 conventions and four-space indentation. Keep code formatted with `cargo fmt --all`; the repository config limits lines to 85 columns. Use `snake_case` for modules, functions, and variables; `CamelCase` for types and traits; and `SCREAMING_SNAKE_CASE` for constants. Keep TOML formatting compatible with `.taplo.toml` and avoid unrelated dependency or generated-file changes.

## Testing Guidelines

Add focused `#[test]` functions in the relevant module (or a nearby `tests` module) and name them for the behavior they verify. Run `cargo test --workspace` locally; include `cargo test --doc --workspace` when documentation or public APIs change. Use the existing Criterion benchmark for performance-sensitive editor changes.

## Commit & Pull Request Guidelines

Recent history uses concise Conventional Commit-style prefixes such as `fix:`, `build:`, `ci:`, `chore:`, and `release:`. Follow that pattern with an imperative, specific subject. Pull requests should explain the change, summarize validation (for example, tests, `cargo fmt`, and Clippy), and link an issue when relevant. Check the PR template and add a `CHANGELOG.md` entry when the change is valuable to users.

### LLM-assisted changes

LLM-assisted contributions follow the same review and verification requirements as other contributions:

- Inspect `git status` and the complete diff before editing or submitting a change. Preserve unrelated user work.
- Keep the change focused. State the scope and non-goals in the pull request description.
- Report only commands that were actually run, and distinguish local results from GitHub Actions results.
- Describe known risks, behavior changes, and follow-up work without overstating their impact.
- Do not push, create or merge a pull request, or delete a remote branch without explicit authorization.
- Use the repository pull request template and keep the title consistent with the Conventional Commit style.
