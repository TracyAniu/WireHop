# Execution Plan: Public Release Hardening

## Goal

Remove the highest-risk blockers to publishing this source snapshot as an independent open-source fork: safe receive paths, non-destructive file writes, bounded metadata, executable regression tests, and distributable third-party notices.

## Context

- `docs/SECURITY.md`
- `docs/TESTING.md`
- `docs/ARCHITECTURE.md`
- `LANDrop/filetransferreceiver.cpp`
- `LICENSE` and `LICENSE.icon`

The harness was committed separately as `af7441e`. The upstream product name and branded artwork cannot be safely reused for the intended independent public release.

## Scope

- Validate every peer-provided filename as a portable leaf name.
- Bound file count, individual file size, and total declared size.
- Receive into temporary files and publish completed files without overwriting existing data.
- Detect short/failed writes and excess data.
- Bound socket buffers and encrypted frames; reject invalid key/ciphertext inputs and repeated pre-approval metadata.
- Add Qt Test coverage for transfer policy and filename collision behavior.
- Replace the intentionally failing test wrapper with the real suite.
- Add a third-party notices file for source redistribution.

## Non-goals

- Choose the new project name, logo, application IDs, or update service.
- Change the wire format or promise compatibility with current closed-source LANDrop clients.
- Migrate from Qt 5 to Qt 6.
- Complete every denial-of-service, protocol-versioning, packaging, signing, or store-distribution task.

## Compatibility and Risk

- Valid v0.4.0 peers remain compatible.
- Previously accepted unsafe, non-portable, extremely large, or over-count metadata will be rejected.
- Existing destination files will no longer be truncated; collisions receive a numbered filename.
- Incomplete transfers remain hidden as temporary files and are removed when the receiver object is destroyed.

## Implementation Steps

- [x] Add a reusable, unit-tested transfer policy for filenames, sizes, totals, and collision paths.
- [x] Integrate the policy and temporary-file lifecycle into `FileTransferReceiver`.
- [x] Add and wire a Qt Test target through `scripts/test.sh`.
- [x] Add third-party notices and update architecture, security, testing, and progress docs.
- [x] Run lint, tests, compilation, native startup smoke, and a fresh adversarial source-review pass.
- [x] Commit the validated hardening change separately from the harness.

## Validation

- [x] `./scripts/lint.sh`
- [x] `./scripts/test.sh`
- [x] `./scripts/typecheck.sh`
- [x] `./scripts/smoke.sh`
- [x] Inspect the complete diff and record unverified peer-to-peer/platform paths.

## Progress Log

- 2026-08-11: Harness committed as `af7441e`; plan created and implementation started.
- 2026-08-11: Added receive-path, overwrite, size, framing, crypto-input, socket-buffer, free-space, and plain-text prompt protections. All 24 Qt tests, lint, a full macOS arm64 build, and native startup smoke passed.
- 2026-08-11: Validated hardening committed separately as `2ddf46c`.

## Open Questions

- New project name, reverse-DNS identifier, artwork, update endpoint, and initial public version remain owner decisions.

## Completion Notes

Implementation and local validation are complete in `2ddf46c`. Existing files are preserved, completed receives use collision-safe names, and the active partial file is automatically removed on session destruction.

Not verified in this environment: a real two-peer transfer, interruption behavior observed end to end, Windows/Linux builds and packaging, or a human security review. The protocol still lacks an explicit version, connection/inactivity limits remain follow-up hardening, and rebranding/release identity work remains blocked on the new name and artwork.
