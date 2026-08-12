# WireHop Agent Guide

## Start Here

WireHop is a C++11/Qt 5 system-tray application for encrypted file transfer over a local network. It is being joined by a Rust core (`core/`) that is a second, independent implementation of the same wire protocol — see `docs/decisions/2026-08-12-rust-core-architecture.md`. The two are held together by `docs/references/PROTOCOL.md` and the conformance fixture beside it, not by shared code. Read only the documentation relevant to the task:

- Product behavior: `docs/PRODUCT.md`
- Code structure and transfer flow: `docs/ARCHITECTURE.md`
- Day-to-day agent workflow: `docs/AI_WORKFLOW.md`
- C++/Qt conventions: `docs/ENGINEERING_STANDARDS.md`
- Validation and manual checks: `docs/TESTING.md`
- Desktop UI work: `docs/FRONTEND.md`
- Networking, cryptography, or file writes: `docs/SECURITY.md`
- Wire protocol (authority for both implementations): `docs/references/PROTOCOL.md`
- Multi-session work: `docs/exec-plans/active/` and `docs/agent-harness/progress.md`

This project is derived from the open-source LANDrop 0.4.0 snapshot. Treat this repository as the authority for WireHop behavior and preserve the upstream copyright and license notices.

## Working Rules

- Check `git status --short` before editing and preserve unrelated user changes.
- Keep changes compatible with the qmake project in `WireHop/WireHop.pro` unless a migration is explicitly in scope.
- Follow the existing Qt parent-ownership and signal/slot patterns.
- Treat discovery packets, transfer metadata, file contents, settings, and update responses as untrusted input.
- Update translations/resources when user-visible strings or assets change.
- Create an execution plan for risky, cross-module, or multi-session work.
- Update the relevant docs when behavior, architecture, validation, or known risk changes.

## Stable Commands

- `./scripts/dev.sh`: build and run WireHop.
- `./scripts/typecheck.sh`: configure and compile the application.
- `./scripts/lint.sh`: check harness shell syntax, JSON, whitespace, and Git diff errors.
- `./scripts/test.sh`: builds and runs the Qt Test suite, then the Rust core suite (`cargo test`).
- `./scripts/smoke.sh`: build, launch the native application briefly, and verify it remains running.
- `./scripts/package-macos.sh`: stage, deploy, ad-hoc sign, verify, launch-check, and zip the macOS package (never run macdeployqt inside the build directory).

Rust steps skip with a loud message when no toolchain is present, so the Qt-only path keeps working; set `WIREHOP_REQUIRE_RUST=1` (as CI does) to make a missing toolchain fatal. Never edit `docs/references/protocol-vectors.json` by hand — regenerate it with `cargo run -p wirehop-cli -- emit-vectors` and review the diff as a wire-protocol change.

The scripts use `build-agent/` by default. Override with `WIREHOP_BUILD_DIR`, `QMAKE_BIN`, or `WIREHOP_JOBS` when needed. Legacy `LANDROP_*` overrides remain accepted for harness compatibility.

## Completion Report

End each task with:

- What changed.
- What validation ran, including intentional skips or unavailable checks.
- What remains risky or unverified.
- The files most relevant for review.
