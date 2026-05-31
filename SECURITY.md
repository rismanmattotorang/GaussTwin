# Security Policy

> GaussTwin is in active **pre-1.0 development**. APIs and security posture are not
> yet stable; do not deploy it in production with untrusted input or data without
> your own review.

## Supported versions

| Version | Supported |
|---|---|
| `0.1.x` (pre-release, `main` / development branches) | ⚠️ Best-effort only |

There is no stable release yet. Security fixes land on the active development
branch.

## Reporting a vulnerability

**Please do not file public GitHub issues for security vulnerabilities.**

Instead, report privately via GitHub's **[Private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)**
("Report a vulnerability" under the repository's **Security** tab), or contact the
maintainers directly.

Please include:
- affected component/crate and version (commit hash),
- a description and impact assessment,
- reproduction steps or a proof of concept,
- any suggested remediation.

We aim to acknowledge reports within a few business days and will coordinate a fix
and disclosure timeline with you.

## Dependency security

Dependency advisories (RUSTSEC) and license policy are enforced in CI via
[`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) using `deny.toml`. To
check locally:

```bash
cargo install cargo-deny
cargo deny check advisories bans licenses sources
```

## Known security-relevant gaps (tracked)

These are documented honestly while pre-1.0 (see `docs/EVALUATION_AND_ROADMAP.md`):

- The codebase still contains many `.unwrap()`/`panic!` calls in library code; a
  Phase 2 sweep is planned to make the runtime panic-free on bad input.
- 42 `unsafe` blocks (mostly SIMD) are pending a documented-invariant audit
  (Phase 2).
- Auth uses `jsonwebtoken` + `argon2`; JWT secret management and full RBAC
  enforcement are not yet hardened for production.
