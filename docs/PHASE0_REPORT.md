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
| `gausstwin-api` | 9 passed, **2 failed** (`test_server_startup`, `test_server_init`) |
| `gausstwin-cosim` | **3 failed** (`test_simulation_time`, `test_fmi_instance`, `test_hla_federate`) + **2 hangs** (`test_conservative_sync`, `test_optimistic_sync` — `SyncManager::synchronize` deadlocks). The two hangs are now `#[ignore]`d with a tracked reason so the suite is runnable. |
| `gausstwin-agent`, `-ai`, `-cli` | 0 lib unit tests (test surface is thin) |
| `db`, `des`, `fsm`, `integration`, `spaces`, `vec`, `visual` | Not yet measured (were blocked behind the cosim hang) |

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
