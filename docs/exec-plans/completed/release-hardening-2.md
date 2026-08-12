# Execution Plan: Release Hardening Round 2

## Goal

Resolve the eight verified review findings that block a stable public release: the Qt 6-unsafe multi-select removal, the broken macOS smoke/startup state, unsigned macOS packages, missing session timeouts and connection caps, unvalidated discovery datagrams, absent sender-side delivery confirmation, SendToDialog socket-lifecycle defects, and a CI pipeline that packages without testing.

## Context

- Review baseline: commit `bf249bc`, clean tree; lint/test(24)/typecheck pass, macOS smoke fails.
- `docs/ARCHITECTURE.md` (transfer flow, protocol compatibility rule at line 67)
- `docs/SECURITY.md` (untrusted-input and overwrite rules)
- `docs/TESTING.md` (regression-test mandate, loopback suite named as next layer)
- Verified findings adjustments:
  - Smoke failure root cause: `build-agent/` was polluted by an in-place `macdeployqt` run (bundle contains Frameworks/PlugIns/qt.conf; `codesign --verify` fails ⇒ AMFI SIGKILL; incremental relink into the polluted bundle ⇒ SIGABRT in QApplication init). Not a rebrand code defect.
  - Discovery `"port": 0` is the designed "not discoverable" beacon (`discoveryservice.cpp:76`) and must stay valid; reject only non-integral/out-of-range values.
  - Old senders silently ignore unknown frames (`filetransfersender.cpp:94-118`), so a receiver completion ACK can be an additive, compatible frame.

## Scope

- Row-index based multi-select removal in SelectFilesDialog.
- Clean-build smoke recovery plus a `_common.sh` guard against macdeployqt-polluted build dirs.
- `scripts/package-macos.sh` staging + ad-hoc codesign + verification; CI macOS job uses it.
- SendToDialog port validation and per-attempt socket teardown/guards.
- Discovery datagram bounds, port parsing, and device-name validation via FileTransferPolicy.
- Session inactivity watchdog with state-aware thresholds; server concurrent-session cap.
- Headless loopback integration tests (session/sender/receiver linked GUI-free) and a CI test job gating packaging.
- Additive receiver completion ACK with honest unconfirmed-send wording on the sender.
- Documentation and translation updates per stage.

## Non-goals

- Developer ID signing or notarization (ad-hoc only this round).
- Protocol version negotiation or any change that breaks LANDrop 0.4.0 interop.
- Authenticating UDP discovery (inherent LAN-trust limitation; documented residual risk).
- Qt 6 migration.

## Compatibility and Risk

- Wire protocol: unchanged through stage 7. Stage 8 adds one receiver→sender encrypted `{"ack":1}` frame after final commit; old senders ignore it (verified), new senders treat missing ACK as qualified success, never error. Compatibility matrix recorded in `docs/ARCHITECTURE.md`.
- UX change (stage 6): sessions idle in handshake >30 s, awaiting accept >300 s, or stalled mid-transfer >60 s are aborted; previously unbounded.
- QObject lifetime: watchdog must never fire after FINISHED; server cap counter relies on receiver destruction — verified in loopback tests.
- Data loss: none expected; receive path already commits via temporary files.
- Platform: signing changes are macOS-only; test job is Ubuntu; Windows packaging untouched.

## Implementation Steps

- [x] Stage 1: Qt 6-safe multi-select removal (`selectfilesdialog.cpp`).
- [x] Stage 2: Clean-build smoke recovery + `_common.sh` polluted-build-dir guard + TESTING.md note.
- [x] Stage 3: `scripts/package-macos.sh` (stage → macdeployqt → ad-hoc sign → verify → launch check → zip); CI macOS job calls it; SECURITY.md residual-risk note.
- [x] Stage 4: SendToDialog port!=0 validation, socket teardown before reuse, sender()-guards in slots.
- [x] Stage 5: `FileTransferPolicy::parsePort`, discovery datagram size bound, device-name validation; policy tests.
- [x] Stage 6: Session watchdog (state-aware, virtual for tests), sender HANDSHAKE2 override, server session cap; zh_CN strings. Manually verified: idle inbound connection aborted at 30.2s; 9th concurrent connection refused while 8 stay served.
- [x] Stage 7: GUI-decoupling (openDownloadFolder signal, downloadPath/deviceName injection), loopback test suite (8 cases, GUI-free), CI test job gating packaging.
- [x] Stage 8: Receiver `{"ack":1}` frame, sender WAITING_FOR_ACK state with 10 s timeout and unconfirmed wording; compat matrix in ARCHITECTURE.md; zh_CN strings. Loopback tests cover ack, no-ack close, and ack-timeout paths.
- [x] Stage 9: Docs sweep, progress log backfill (rebrand/packaging entries), move this plan to completed/.

## Validation

- [x] `./scripts/lint.sh` per stage.
- [x] `./scripts/typecheck.sh` per stage.
- [x] `./scripts/test.sh` per stage (24 baseline grown to 35: policy ports, 10-case loopback suite).
- [x] `./scripts/smoke.sh` per stage (red at baseline, green from stage 2 onward).
- [x] `codesign --verify --deep --strict` passes on the packaged app (stage 3, locally and in CI).
- [x] CI: test job runs lint+test and gates the three package jobs (verified green on the stage-7 push, run 31563716465).
- [x] Manual: idle connection aborted at 30.2s; 9th concurrent connection refused; packaged app launch-checked. Automated loopback tests cover accepted/rejected/disconnect/ack transfer paths; multi-select removal and port-entry UI paths remain manually exercised via dialog usage only.

## Progress Log

- 2026-08-12: Plan created from verified review findings; baseline recorded (smoke red, cause identified as polluted build dir). Next: stage 1.
- 2026-08-12: Stage 1 done (row-index descending removal). Stage 2 done: after `rm -rf build-agent`, a full clean rebuild and the startup smoke passed, confirming the polluted-build-dir root cause; added the `configure_wirehop` qt.conf guard and the TESTING.md rule. Next: stage 3 (package-macos.sh + ad-hoc signing).
- 2026-08-12: Stage 3 done. `package-macos.sh` stages into `dist-macos/`, re-signs ad-hoc after macdeployqt, passes `codesign --verify --deep --strict`, launch-checks the packaged app, and zips; CI macOS job now calls it. Local run green; smoke re-run confirms `build-agent/` stays clean. CI verification deferred to the stage-7 branch push. Next: stage 4 (SendToDialog).
- 2026-08-12: Stages 4-6 done and committed (SendToDialog lifecycle, discovery validation, watchdog + session cap with manual timing/cap verification). Stage 7 done: GUI decoupling, 8-case loopback suite, CI test job; branch pushed and the full workflow (test + three package jobs) passed in 2m33s. Stage 8 done: additive receiver ACK with sender WAITING_FOR_ACK, two more loopback cases (10 total), zh_CN strings. Closing out.

## Open Questions

- Watchdog thresholds (30 s / 300 s / 60 s / ACK 10 s) are defaults; adjust if user feedback prefers different budgets.

## Completion Notes

All eight review findings resolved across commits f17a918..HEAD on branch `release-hardening-2`. The macOS smoke failure was environmental (macdeployqt had polluted `build-agent/`), now guarded against in `_common.sh` and structurally prevented by `package-macos.sh` staging into `dist-macos/`. Packages are ad-hoc signed and verified; the delivery ACK is additive and LANDrop 0.4.0-compatible both ways (matrix in ARCHITECTURE.md). The Qt Test suite grew from 24 to 35 cases including a GUI-free loopback integration suite, and CI now gates all packaging on lint+tests (verified green).

Remaining risks and follow-ups: discovery remains unauthenticated by design (six-digit code is the real peer check); packages are ad-hoc signed only — Developer ID/notarization still needed for frictionless distribution; the protocol still has no version field; dialog-layer UI behavior (SelectFilesDialog removal, SendToDialog entry validation) is compile-covered but not UI-automated; Windows/Linux runtime behavior is CI-built but not manually exercised.
