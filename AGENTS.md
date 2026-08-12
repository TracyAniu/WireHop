# WireHop Agent Guide

## Start Here

WireHop is a C++11/Qt 5 system-tray application for encrypted file transfer over a local network. Read only the documentation relevant to the task:

- Product behavior: `docs/PRODUCT.md`
- Code structure and transfer flow: `docs/ARCHITECTURE.md`
- Day-to-day agent workflow: `docs/AI_WORKFLOW.md`
- C++/Qt conventions: `docs/ENGINEERING_STANDARDS.md`
- Validation and manual checks: `docs/TESTING.md`
- Desktop UI work: `docs/FRONTEND.md`
- Networking, cryptography, or file writes: `docs/SECURITY.md`
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
- `./scripts/test.sh`: builds and runs the Qt Test security and transfer-policy regression suite.
- `./scripts/smoke.sh`: build, launch the native application briefly, and verify it remains running.
- `./scripts/package-macos.sh`: stage, deploy, ad-hoc sign, verify, launch-check, and zip the macOS package (never run macdeployqt inside the build directory).

The scripts use `build-agent/` by default. Override with `WIREHOP_BUILD_DIR`, `QMAKE_BIN`, or `WIREHOP_JOBS` when needed. Legacy `LANDROP_*` overrides remain accepted for harness compatibility.

## Completion Report

End each task with:

- What changed.
- What validation ran, including intentional skips or unavailable checks.
- What remains risky or unverified.
- The files most relevant for review.
