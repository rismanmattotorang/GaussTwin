# Changelog

All notable changes to GaussTwin are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/) once
it reaches `1.0.0`. While pre-1.0, minor versions may include breaking changes.

## [Unreleased]

### Added
- **CI pipeline** (`.github/workflows/ci.yml`): rustfmt, build, tests, `cargo-deny`
  (advisories + licenses), clippy, rustdoc, and coverage. Blocking vs. advisory
  gates with a documented ratchet policy.
- Project governance: `CONTRIBUTING.md`, `SECURITY.md`, this `CHANGELOG.md`.
- Build & toolchain scaffolding: `rust-toolchain.toml` (pinned 1.94.1),
  `rustfmt.toml`, `deny.toml`, a real `.gitignore`.
- Documentation: `docs/BUILD.md` (prerequisites + build matrix),
  `docs/PHASE0_REPORT.md` (honest build/test baseline),
  `docs/EVALUATION_AND_ROADMAP.md` (production-readiness plan).
- `AgentId::from_raw(u128)` — deterministic ID constructor for reproducible
  scenarios and tests.
- `torch` Cargo feature on `gausstwin-ai` gating the libtorch-backed `ml` module.

### Changed
- **Build is now green from a clean checkout** with light default features. Heavy
  backends are opt-in.
- Whole codebase formatted with `rustfmt`.
- README build instructions corrected; status reporting now points to the
  CI-verified baseline instead of aspirational percentages.

### Fixed
- `gausstwin-core` no longer fails to compile (2 borrow-checker errors in
  `profiler.rs`).
- `gausstwin-core` tests: 80 unit + 1 doc test green (was 75/5). Real bugs fixed:
  - `AgentArena::deallocate` reuses freed slots LIFO (immediate reuse).
  - `ThreadPool::wait_idle` waits for queued work (counts outstanding tasks from
    submit time).
  - `FederatedLearning` averaging sizes from incoming updates (no longer empty).
  - `NeuralAgent` activation off-by-one (output layer applies its configured
    activation).
  - `profiler` construction no longer clobbers the global enable switch (removed
    flaky cross-test global state).
- `gausstwin-api`: `MetricsManager` installs the process-global Prometheus recorder
  exactly once (was failing on the second `AppState` construction). 11 tests green.
- `gausstwin-cosim`: stale `SyncMode` test constructions updated to compile.
- `gausstwin-des`: checkpointing now produces checkpoints for short runs and tracks
  time since the last checkpoint (was gated on wall-clock-since-start).
- `gausstwin-integration`: Ethereum mock `deploy_contract` no longer panics on
  `1u128 << 160` shift overflow.
- `gausstwin-db`: crate-level rustdoc example imports `DatabaseError`; the
  live-SurrealDB integration test is `#[ignore]`d (run with `--ignored`).
- `gausstwin-fsm`: fixed the crate-level rustdoc doctest (State API drift).

### CI green set
- Blocking test set expanded to {core, api, fsm, des, integration, db, spaces, vec,
  visual}. Only `cosim` (deadlock) and `data` (tests don't compile) remain.

### Phase 2 (determinism)
- `StandardModel` now seeds its random scheduler from `ModelConfig::seed`
  (`with_seed` was previously ignored — the scheduler used `rand::random()`).
- `AgentSet::agent_ids()` returns a deterministic (sorted) order instead of
  `HashMap` key order; `AgentId` is now `Ord`.
- Added a scheduler seed-stability test (same seed ⇒ identical activation order).
  Note: end-to-end state-trace determinism is still blocked on the agent-execution
  loop, which `StandardModel::step` does not yet implement.

### Phase 2 (started) — keep core minimal
- Feature-gated the speculative `gausstwin-core` modules (`hpc`, `gpu`,
  `distributed`, `quantum`, `blockchain`) behind opt-in Cargo features. The default
  build is now the minimal hardened core (62 tests); `--features experimental`
  enables the CPU-testable set (75 tests); `gpu` is separate (needs a GPU adapter).
  These modules aren't referenced by any other crate, so the default surface
  shrinks with no downstream impact. CI tests both default and `experimental`.

### Removed
- Dead dependencies: `milvus-sdk-rust` (unused; also forced `protoc`), `smartcore`,
  `ndarray-linalg` (forced system OpenBLAS).
- Committed `.DS_Store` files.

### Known issues / backlog
- `gausstwin-data` test suite is mid-migration (does not compile yet).
- `gausstwin-cosim` has runtime test failures and a `synchronize()` deadlock (two
  tests `#[ignore]`d with a tracked reason).
- Many library `.unwrap()`/`panic!` calls and 42 `unsafe` blocks await Phase 2.

[Unreleased]: https://github.com/rismandev/gausstwin/commits/gauss-twin
