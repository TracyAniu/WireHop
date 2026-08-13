# Execution Plan: macOS Services Entry and External File Intake

## Goal

Right-clicking file(s) in Finder shows "Send with WireHop" under Services (localized to Chinese), which opens the send dialog preloaded with those files. The app also accepts files via macOS open events and command-line arguments on all platforms.

## Context

- `WireHop/main.cpp` has no QFileOpenEvent handling and ignores argv.
- `TrayIcon::sendActionTriggered` (`trayicon.cpp:93-98`) is the dialog-creation pattern to reuse.
- `SelectFilesDialog::addFile` is private and not directory-aware; drag&drop intake exists (`selectfilesdialog.cpp:139-154`).
- `WireHop/Info.plist` is hand-maintained (`QMAKE_INFO_PLIST`); no NSServices yet. `LSUIElement` is true, so dialogs must be raised explicitly.
- No `.mm` files or AppKit linkage exist yet; qmake `macx` block must be added.
- Services register only when LaunchServices indexes the bundle; running the raw binary (dev.sh) does not register them.

## Scope

- `SelectFilesDialog`: public `addFiles(QStringList)`, directory rejection in `addFile`.
- `TrayIcon`: public `sendFiles(QStringList)` opening a preloaded dialog (raise + activate).
- `main.cpp`: FileOpen event collector (coalesces multi-file opens), CLI file arguments, wiring.
- `WireHop/macservices.h/.mm`: NSServices provider calling `TrayIcon::sendFiles` on the Qt main loop.
- `Info.plist`: `NSServices` entry (`NSSendFileTypes: public.data`), `CFBundleName`.
- `WireHop/locales/zh-Hans.lproj/InfoPlist.strings`: localized menu title via `QMAKE_BUNDLE_DATA`.
- Docs: ARCHITECTURE external interfaces, TESTING manual workflow, PRODUCT feature note.

## Non-goals

- ~~A true Share-sheet extension (.appex) — deferred until Developer ID signing exists.~~ Scope extended: the feasibility spike proved ad-hoc appex loading works on macOS 15, so the Share extension is now in scope (see progress log).
- Single-instance guard for double launches of the raw binary (Finder/Services always route to the running bundle instance).
- Windows/Linux shell integration.

## Compatibility and Risk

- No wire protocol or transfer-path changes; loopback tests unaffected.
- New platform code is macOS-only behind `Q_OS_MACOS`/`macx`; Linux/Windows builds unchanged except harmless CLI intake.
- QObject lifetime: the services provider holds a TrayIcon pointer; TrayIcon outlives the event loop (stack object in main), provider is only invoked while the loop runs.

## Implementation Steps

- [x] SelectFilesDialog addFiles + directory guard.
- [x] TrayIcon::sendFiles.
- [x] main.cpp collector + CLI intake + wiring.
- [x] macservices provider, qmake macx block, AppKit linkage.
- [x] Info.plist NSServices + CFBundleName; zh-Hans InfoPlist.strings via QMAKE_BUNDLE_DATA.
- [x] Docs updates (ARCHITECTURE interfaces, TESTING workflow 7, PRODUCT).

## Validation

- [x] `./scripts/lint.sh`, `./scripts/typecheck.sh`, `./scripts/test.sh` (35 passing), `./scripts/smoke.sh`.
- [x] Bundle verified: NSServices present in built Info.plist; zh-Hans.lproj copied into Resources.
- [x] Service registered via lsregister + pbs; listed in `pbs -dump_pboard`; programmatic `NSPerformService("Send with WireHop", pboard)` returned true and launched the app.
- [x] Finder right-click visual confirmation — performed by the owner on 2026-08-12. It surfaced a real defect rather than passing quietly: the dialog opened but never came to the front, because an LSUIElement application is outside the normal activation order. Fixed in `2f4ceee`. The zh-Hans menu title remains visually unconfirmed.

## Progress Log

- 2026-08-12: Plan created after exploration; implementation starting.
- 2026-08-12: Implemented and machine-verified end-to-end (registration + NSPerformService launch). Discovered and guarded a qmake pitfall: the generated bundle Info.plist has no dependency on the source plist, so `_common.sh` now deletes the stale copy when the source is newer.
- 2026-08-13: Closed. The final open item — Finder right-click confirmation — was exercised by the owner and found the activation defect described above; `CGWindowListCopyWindowInfo` and `NSWorkspace` proved the dialog existed at full opacity while another application held the front, which is why it read as "nothing happened". Two environmental factors were identified and left to configuration rather than code: a three-display setup places a parentless dialog on the screen under the cursor, and a menu-bar manager (Bartender) hides the status item that is this app's only entry point.
- 2026-08-12: User reported the Finder attempt failed with "cannot open the specified document or URL" — root cause: no `CFBundleDocumentTypes`, so LaunchServices refused document-open requests. Fixed (viewer role, rank None, public.data). Share-sheet spike succeeded: a hand-built, ad-hoc-signed appex (clang + `-e _NSExtensionMain`) registers and enables in pluginkit on macOS 15 — but only when signed with its sandbox entitlements AFTER any `--deep` app signature (codesign --deep strips nested entitlements; this cost the first spike round). Productionized: `WireHop/shareext/`, `scripts/build-share-extension.sh`, QMAKE_POST_LINK embedding, corrected signing order in `package-macos.sh`. Verified: `open -a` document path now succeeds and launches the app (previously the exact reported error); full packaging green.

## Open Questions

- None.

## Completion Notes

Feature complete. Implemented on branch `macos-share-services`; the intake chain was finally exercised end-to-end through Finder on 2026-08-13, which is what exposed the activation defect fixed in `2f4ceee`. The intake chain is Finder Services / QFileOpenEvent / CLI args → `TrayIcon::sendFiles` → preloaded `SelectFilesDialog` (directories rejected, existing dedupe and open checks reused). Programmatic service invocation verified; the Finder visual click and Chinese menu title remain user-confirmed. Residual notes: services route to the registered bundle — during development re-run `lsregister -f` after moving the app; the true Share-sheet (.appex) remains deferred until Developer ID signing exists.
