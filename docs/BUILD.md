# Building GaussTwin

This document is the source of truth for building GaussTwin from a clean checkout.
It reflects the **Phase 0** build cleanup (see `docs/EVALUATION_AND_ROADMAP.md`).

## TL;DR

```bash
# Default build — light, hermetic, no network-fetched native blobs:
cargo build --workspace
cargo test  --workspace
```

The default build covers all 13 core crates plus the CLI. Heavy/optional backends
are **opt-in via Cargo features** so the default build stays fast and reliable.

## Toolchain

The toolchain is pinned in `rust-toolchain.toml` (currently **1.94.1**). `rustup`
will install it automatically on first build. `rustfmt` and `clippy` are included.

## System prerequisites

The default build needs a C toolchain and a few common native libraries (pulled
by transitive dependencies such as `openssl-sys`, `aws-lc-sys`, and `cmake`-built
crates):

| Tool / lib | Why | Debian/Ubuntu |
|---|---|---|
| C/C++ compiler | `*-sys` crates, `cmake` builds | `build-essential` |
| `cmake` | `aws-lc-sys`, `librdkafka`, etc. | `cmake` |
| `pkg-config` | locating system libs | `pkg-config` |
| OpenSSL headers | `openssl-sys` | `libssl-dev` |
| Cyrus SASL headers | `sasl2-sys` via `rdkafka` (Kafka connector) | `libsasl2-dev` |
| `protoc` (Protocol Buffers) | **gRPC** codegen in `gausstwin-api` (`tonic-build`) | `protobuf-compiler` |

```bash
sudo apt-get update && sudo apt-get install -y \
    build-essential cmake pkg-config libssl-dev libsasl2-dev protobuf-compiler
```

> Several heavy connectors in `gausstwin-integration` (Kafka SASL/SSL, etc.) pull
> native `-sys` crates. Phase 3 of the roadmap will feature-gate individual
> connectors so a minimal build needs none of these; for now they are required by
> the default build of that crate.

> **Note on `protoc`:** it is required only because `gausstwin-api` compiles gRPC
> `.proto` files at build time. If you do not need gRPC you can build a subset,
> e.g. `cargo build -p gausstwin-core -p gausstwin-spaces -p gausstwin-des`.

## Optional features

| Feature | Crate | Enables | Extra requirement |
|---|---|---|---|
| `torch` | `gausstwin-ai` | libtorch-backed neural-net `ml` module + `AISystem` model factory | libtorch (~2GB, downloaded by `tch` from `download.pytorch.org`) |

```bash
# Build with the libtorch ML stack (needs network access to download.pytorch.org):
cargo build -p gausstwin-ai --features torch
```

> The hand-rolled `tch` ML stack is slated to be replaced by **Candle** in Phase 3
> of the roadmap, which will remove the libtorch system dependency entirely.

## What was changed in Phase 0 (build integrity)

- **`gausstwin-core`** — fixed 2 borrow-checker (`E0515`) errors in `profiler.rs`
  that prevented the foundational crate (and therefore the whole workspace) from
  compiling.
- **`tch` is now optional** (`torch` feature, off by default). Previously it was a
  hard dependency of `gausstwin-ai` — which every crate transitively depends on —
  so the entire workspace required a ~2GB network-fetched libtorch just to build.
- **Removed dead dependencies:** `milvus-sdk-rust 0.1.0` (declared but never
  imported; it also forced `protoc`), plus the unused `smartcore` and
  `ndarray-linalg` (the latter required system OpenBLAS).
- **Bindings excluded from the default workspace.** `gausstwin-py` (pyo3) and
  `gausstwin-ts` (wasm) need dedicated toolchains and pull `tch` directly. Build
  them explicitly (see below).
- Added `rust-toolchain.toml`, `rustfmt.toml`, `deny.toml`, and a real `.gitignore`;
  removed committed `.DS_Store` files.

## Language bindings (separate toolchains)

```bash
# Python (requires: pip install maturin, and libtorch for the torch feature)
cd bindings/gausstwin-py && maturin develop --features torch

# TypeScript / WASM (requires: cargo install wasm-pack)
cd bindings/gausstwin-ts && wasm-pack build --target web
```

## User interfaces

```bash
cd ui/web      && npm install && npm run dev          # React web UI
cd ui/desktop  && npm install && npm run tauri dev    # Tauri desktop app
cd ui/tui      && cargo run --release                 # Ratatui terminal UI
```

## Quality gates (run what CI runs)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check          # requires: cargo install cargo-deny
```
</content>
