# Building, flashing and debugging

## Toolchain (already installed on this machine)

| Tool | Version | Purpose |
|---|---|---|
| rustc / cargo | 1.99.1 | Host + cross compilation |
| Rust targets | — | `x86_64-apple-darwin` (Mac), `aarch64-unknown-linux-gnu` (Linux/Pi 5) |

Install a target:

```bash
rustup target add x86_64-apple-darwin aarch64-unknown-linux-gnu
```

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run --release
```

## Verify

```bash
cargo test
```

## Debug

### Logging

Run the engine with `--features custom-protocol` and connect via the UI's
dev server. Logs appear on stdout.

## Common failures

| Symptom | Cause |
|---|---|
| `cargo test` fails with linking errors | Missing target: `rustup target add aarch64-unknown-linux-gnu` |
| Dev build loads dev URL | Run `tauri build --features custom-protocol` (see AGENTS.md) |
