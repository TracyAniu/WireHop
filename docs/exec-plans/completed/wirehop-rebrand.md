# Execution Plan: WireHop Rebrand

## Goal

Turn the maintained fork into an independently branded WireHop 0.1.0 source tree and application without carrying LANDrop product identity or restricted LANDrop icon artwork into future releases.

## Decisions

- Product name: `WireHop`
- Command/executable: `wirehop`
- Initial independent version: `0.1.0`
- Application identifier: `io.github.tracyaniu.wirehop`
- Repository target after owner-approved GitHub rename: `TracyAniu/WireHop`
- Tagline: `Files, one hop away.`
- Preserve LANDrop copyright and BSD attribution; distinguish WireHop modifications.
- Preserve compatible LANDrop 0.4.0 settings on first launch through a one-time migration.

## Scope

- Replace application and banner artwork with original WireHop assets.
- Rename the source directory, qmake project, translation catalog, desktop entry, binaries, packages, scripts, tests, and CI paths.
- Replace runtime application metadata, macOS bundle identifier, update endpoint, temporary-file prefix, and user-facing brand text.
- Remove the old icon license after all covered artwork is removed, while documenting its historical removal.
- Rewrite current product documentation while preserving historical attribution and completed-plan facts.
- Validate brand residue, translations, unit tests, full compilation, and native startup.

## Non-goals

- Change the file-transfer wire protocol or intentionally break LANDrop 0.4.0 peer compatibility.
- Rename the GitHub repository without explicit approval for that separate external action.
- Produce signed/notarized/store-ready release binaries.

## Steps

- [x] Inventory all product-name, identifier, URL, path, package, and artwork references.
- [x] Generate, inspect, and integrate original WireHop icon/banner assets.
- [x] Rename build/runtime/platform artifacts and add legacy settings migration.
- [x] Replace the upstream update service with the future WireHop GitHub Releases endpoint.
- [x] Update documentation, notices, tests, and Simplified Chinese translations.
- [x] Run lint, unit tests, compilation, native smoke, and brand-residue review.
- [x] Commit the validated rebrand and archive this plan.

## Validation

- [x] `./scripts/lint.sh`
- [x] `./scripts/test.sh`
- [x] `./scripts/typecheck.sh`
- [x] `./scripts/smoke.sh`
- [x] No runtime/package path references the old LANDrop product identity.
- [x] Remaining `LANDrop` references are limited to historical copyright, provenance, compatibility, or migration logic.

## Progress

- 2026-08-11: WireHop name approved; rebrand inventory and asset work started.
- 2026-08-11: Original WireHop icon, mask, banner, ICO, and ICNS assets generated and visually verified.
- 2026-08-11: Runtime identity, paths, scripts, tests, packaging, documentation, and translation resources renamed to WireHop 0.1.0.
- 2026-08-11: Lint passed; 24 Qt tests passed; the macOS application compiled and remained running through the native startup smoke window.

## Completion Notes

The WireHop visual direction was generated from an original two-device hop concept, then rebuilt as deterministic SVG and platform icon assets. The first launch migrates compatible LANDrop 0.4.0 preferences once without changing the wire protocol. macOS was compiled and smoke-tested locally; Linux and Windows packaging remain CI-only and unverified in this environment. Rename the GitHub repository to `TracyAniu/WireHop` before relying on the update URL, badge, clone URL, or publishing release artifacts. Record the final commit ID in the task handoff.
