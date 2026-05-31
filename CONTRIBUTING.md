# Contributing to GaussTwin

Thanks for your interest in GaussTwin! This guide covers how to build, test, and
submit changes.

## Prerequisites & building

See **[docs/BUILD.md](docs/BUILD.md)** for the toolchain, system dependencies, and
build commands. In short:

```bash
# Pinned toolchain installs automatically via rust-toolchain.toml (1.94.1).
sudo apt-get install -y build-essential cmake pkg-config libssl-dev libsasl2-dev protobuf-compiler
cargo build --workspace
cargo test --workspace
```

## MSRV (Minimum Supported Rust Version)

The toolchain is **pinned in `rust-toolchain.toml`** (currently `1.94.1`), and that
is the version CI uses. The workspace also declares `rust-version = "1.70"` as the
nominal MSRV; bumping either is a deliberate change that must be called out in the
PR description and `CHANGELOG.md`.

## Quality gates (what CI enforces)

Run these locally before opening a PR — they mirror `.github/workflows/ci.yml`:

```bash
cargo fmt --all --check                 # formatting (BLOCKING)
cargo build --workspace                 # build (BLOCKING, warnings = errors)
cargo test -p gausstwin-core -p gausstwin-api   # green-crate tests (BLOCKING)
cargo clippy --workspace --all-targets  # lint (advisory today; being ratcheted)
cargo deny check                        # licenses + RUSTSEC advisories (BLOCKING)
```

### Gate status & ratcheting

CI distinguishes **blocking** gates (must pass) from **advisory** gates
(`continue-on-error`, visible but non-blocking). Advisory gates exist where a known
backlog is being worked down — see `docs/PHASE0_REPORT.md`:

- **Full-workspace tests** are advisory until `gausstwin-data`'s test migration and
  the `gausstwin-cosim` runtime fixes land. As each crate goes green, add it to the
  blocking `build-test` job and remove it from the advisory exclusions.
- **`clippy -D warnings`** is advisory until the existing warnings are cleared
  (Phase 2). Don't *add* new warnings.

The rule of thumb: **the blocking set only grows.** Don't regress a gate that is
already green.

## Commit & PR conventions

- Branch from the current development branch; keep PRs focused.
- Write descriptive commit messages explaining the *why*, not just the *what*.
- Don't commit generated artifacts, `target/`, or editor/OS files (see `.gitignore`).
- New behavior needs tests. Bug fixes should include a regression test.
- If you change a public API, update the rustdoc and `CHANGELOG.md`.

## Heavy / optional features

Heavy backends are feature-gated so the default build stays light:

- `gausstwin-ai` neural stack (libtorch) is behind the `torch` feature (off by
  default). See `docs/BUILD.md`.
- Speculative modules (`gpu`, `quantum`, `blockchain`, `distributed`) are slated to
  move behind `experimental` features / out of `gausstwin-core` in Phase 2.

## Reporting security issues

See [SECURITY.md](SECURITY.md) — please do **not** open public issues for
vulnerabilities.
