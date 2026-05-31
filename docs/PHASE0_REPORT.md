# Phase 0 Completion Report — Build Integrity & Baseline

> **Date:** 2026-05-31  •  **Branch:** `gauss-twin`
> **Goal (from `docs/EVALUATION_AND_ROADMAP.md`):** make the workspace build green
> from a clean checkout on a pinned toolchain, feature-gate heavy backends, and
> freeze an honest baseline.

## Outcome

| Phase 0 exit-gate item | Status |
|---|---|
| `cargo check --workspace` green from clean checkout | ✅ **Achieved** (0 errors, all 13 crates + CLI) |
| Foundational crate compiles | ✅ Fixed 2 `E0515` borrow errors in `gausstwin-core` |
| Heavy backends opt-in via features | ✅ `tch`/libtorch now behind `torch` feature (off by default) |
| Pinned toolchain + system deps documented | ✅ `rust-toolchain.toml` + `docs/BUILD.md` |
| Lint/format/audit config committed | ✅ `rustfmt.toml`, `deny.toml`, real `.gitignore` |
| `cargo test --workspace` green | ⚠️ **Partial** — see "Test baseline" below |

**The primary Phase 0 blocker is resolved:** the project went from *not compiling at
all* (and requiring a ~2GB network-fetched libtorch + undeclared `protoc`/`libsasl2`)
to a **light, hermetic, green default build**.

## What changed

- **`gausstwin-core`** — fixed 2 borrow-checker errors in `profiler.rs`.
- **`tch` made optional** (`torch` feature). It was a hard dependency of
  `gausstwin-ai`, which the whole workspace depends on, so libtorch was required
  just to compile. Gated the `ml` module + `AISystem` model factory accordingly.
- **Removed dead dependencies:** `milvus-sdk-rust 0.1.0` (never imported; forced
  `protoc`), `smartcore`, `ndarray-linalg` (forced system OpenBLAS).
- **Bindings excluded** from the default workspace (pyo3/wasm need their own
  toolchains and pull `tch`); documented separate build steps.
- **Scaffolding:** `rust-toolchain.toml` (1.94.1), `rustfmt.toml`, `deny.toml`,
  real `.gitignore`, removed committed `.DS_Store` files, `docs/BUILD.md`, fixed
  README build instructions.
- **`gausstwin-cosim`** — updated stale `SyncMode` test constructions so tests compile.

## Test baseline (honest snapshot)

`cargo test --workspace` does **not** yet pass. The failures are **pre-existing**
(test code and runtime bugs that predate the Phase 0 cleanup), now made visible by a
building workspace. They form the input backlog for **Phase 1 (test hardening)**.

### A. Test-code compile drift (library builds, tests don't)
APIs evolved but `#[cfg(test)]` modules were never updated:

| Crate | Test errors | Representative drift |
|---|---|---|
| `gausstwin-core` | 19 (across agent/spatial/ai/viz/pool/blockchain/streaming) | `AgentId::from_raw` removed; `VecN` changed from custom enum to `Vector3` alias (`::Vec2`/`new_3d` gone); `Value::Integer/String/Float` variants renamed |
| `gausstwin-data` | 64 (12 lib + 52 integration) | `CacheConfig.refresh_ahead`, `MetricsConfig.interval`, `ScalarData.fields`, `SearchResult.combined_score` removed; `QueryFilters` fields renamed; missing `.await` on async pool API; `ComparisonOperator` moved |

### B. Runtime failures / hangs (tests compile and run, but fail)

| Crate | Result |
|---|---|
| `gausstwin-api` | ✅ **Fixed** — 11 passed, 0 failed (see "Resolved" below) |
| `gausstwin-cosim` | **3 failed** (`test_simulation_time`, `test_fmi_instance`, `test_hla_federate`) + **2 hangs** (`test_conservative_sync`, `test_optimistic_sync` — `SyncManager::synchronize` deadlocks). The two hangs are now `#[ignore]`d with a tracked reason so the suite is runnable. *(Deferred to Phase 1.)* |
| `gausstwin-agent`, `-ai`, `-cli` | 0 lib unit tests (test surface is thin) |
| `db`, `des`, `fsm`, `integration`, `spaces`, `vec`, `visual` | Not yet measured (were blocked behind the cosim hang) |

### Resolved in the Phase 0 follow-up pass

**`gausstwin-core` — now 80 unit + 1 doc test green (was 75/5), stable across runs:**
- Test-code drift fixed: restored `AgentId::from_raw` as a documented deterministic
  constructor; migrated `VecN` call sites to the 3D `Vector3` API.
- Four **real bugs** found once the tests could run:
  - `AgentArena::deallocate` now reuses freed slots LIFO (immediate reuse).
  - `ThreadPool::wait_idle` no longer returns before queued work runs (outstanding
    tasks are now counted from submit time, not worker pick-up).
  - `FederatedLearning` averaging sizes the aggregate from incoming updates (was
    always empty because the global model starts with no weights).
  - `NeuralAgent` off-by-one activation mapping fixed (output layer now applies its
    configured activation, e.g. Sigmoid, instead of Linear).
  - `profiler` no longer clobbers the global enable switch at construction (removed
    cross-test global-state pollution that made `test_basic_timing` flaky).

**`gausstwin-api` — now 11 tests green (was 9/2):**
- `MetricsManager::new` installed the **process-global** Prometheus recorder on
  every call; the second `AppState` construction failed with "a recorder has
  already been installed". Now installs once (OnceLock + double-checked lock) and
  reuses the cached handle — a real production bug, not just a test issue.

### Full workspace test baseline (measured 2026-05-31, Phase 1)

`cargo test --workspace --exclude gausstwin-data` (data tests don't compile):

| Crate | Result | Notes |
|---|---|---|
| `gausstwin-core` | ✅ 80 pass | green (blocking in CI) |
| `gausstwin-api` | ✅ 11 pass | green (blocking in CI) |
| `gausstwin-fsm` | ✅ 9 pass | green (blocking in CI) |
| `gausstwin-des` | ✅ 5 pass | green (blocking in CI) — was 1 fail, fixed |
| `gausstwin-integration` | ✅ 67 pass | green (blocking in CI) — was 1 fail, fixed |
| `gausstwin-db` | ✅ compiles (1 integration test `#[ignore]`d) | blocking in CI; test needs live SurrealDB |
| `gausstwin-agent` | ✅ 0 tests | compiles; no unit tests |
| `gausstwin-ai` | ✅ 0 tests | compiles; no unit tests (torch off) |
| `gausstwin-cli` | ✅ 0 tests | compiles; no unit tests |
| `gausstwin-cosim` | ❌ 4 pass, **3 fail**, 2 ignored | + `synchronize()` deadlock (ignored) |
| `gausstwin-spaces` | ❌ **2 fail** (`test_memory_pool`, `test_octree`) + 1 hang | `test_spatial_cache` hangs → now `#[ignore]`d |
| `gausstwin-vec` | ❔ unmeasured | was blocked behind the spaces hang |
| `gausstwin-visual` | ❔ unmeasured | was blocked behind the spaces hang |
| `gausstwin-data` | ⛔ tests don't compile | 64 errors vs. evolved store API |

**Green test set (CI blocking) = {core, api, fsm, des, integration, db}** plus
agent/ai/cli (compile, no tests). The blocking set only grows as the backlog clears.

#### Fixes that cleared the three 1-failure crates (Phase 1)
- `des::test_checkpointing` — checkpoints were gated on wall-clock-since-start ≥
  interval, so sub-interval runs produced none; now tracks time since the last
  checkpoint and always leaves a final checkpoint when enabled.
- `integration::blockchain::ethereum::test_deploy_contract` — `1u128 << 160`
  overflowed (shift ≥ 128 bits) and panicked in debug; removed the meaningless
  modulo (the `{:040x}` format already yields a 20-byte address).
- `db::test_enterprise_features` — connects to a live SurrealDB; `#[ignore]`d with a
  reason (run with `--ignored`; Phase 3 wires testcontainers).

### Remaining Phase 1 backlog (ratchet into the blocking set as fixed)
1. `gausstwin-spaces` — 2 failures + the `test_spatial_cache` hang (now ignored).
2. `gausstwin-cosim` — 3 failures + the `synchronize()` deadlock (ignored).
3. `gausstwin-vec`, `gausstwin-visual` — measure once spaces no longer hangs.
4. `gausstwin-data` — migrate the 64-error test suite to the current store API.

> Two hangs are now `#[ignore]`d with tracked reasons (`cosim::synchronize`,
> `spaces::SpatialCache`) so `cargo test` completes instead of stalling — these are
> **real runtime bugs**, not test issues.

### Implications
- Several "✅ complete / 80–95%" claims in `PROGRESS.md` are **not** backed by passing
  tests. Status reporting should be driven by CI test results (Phase 1).
- `gausstwin-data`'s 52-error integration test references a substantially changed
  store API; modernizing it requires reconstructing intended behavior and should be
  done deliberately (not mechanically) — a Phase 1 task.
- The `cosim` `synchronize` deadlock is a **real runtime bug**, not test drift.

## Recommended next step

Proceed to **Phase 1**: stand up CI (so this baseline can only improve), then work
the test backlog above crate-by-crate, starting with `gausstwin-core` (foundational)
and the `api` server tests, then the `cosim` deadlock, then `gausstwin-data`.
</content>
