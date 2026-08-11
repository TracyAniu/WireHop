# Engineering Standards

## C++ and Qt

- Target the C++11 and Qt 5 baseline declared in `LANDrop/LANDrop.pro` unless a migration is explicitly approved.
- Match the existing four-space indentation, brace placement, include grouping, and class/file naming.
- Prefer Qt value/container types at Qt API and serialization boundaries.
- Use QObject parent ownership or an existing smart-pointer pattern; avoid ambiguous lifetime and double ownership.
- Preserve event-loop responsiveness. File and network work must remain incremental/asynchronous rather than blocking UI callbacks.
- Keep signal/slot connections close to the object that owns the interaction.
- Edit `.ui`, `.qrc`, and `.ts` sources, never qmake-generated `ui_*.h`, `moc_*`, or `qrc_*` files.

## Protocol and State Machines

- Validate state before consuming a message or acting on a socket callback.
- Treat every length, JSON field, address, port, filename, and declared file size from a peer as untrusted.
- Preserve the framing limit: encrypted messages are prefixed by a two-byte length. Check size arithmetic before narrowing to `quint16` or converting signed values.
- Make terminal success, rejection, socket error, and cleanup paths explicit and idempotent.
- Protocol changes must document compatibility, rollout/versioning, and failure behavior in an architecture decision.

## Errors and User Feedback

- Surface actionable errors through the existing session signals or top-level exception handling.
- Do not silently continue after partial file writes, invalid metadata, cryptographic failures, or socket errors.
- Avoid exposing secrets or raw sensitive contents in dialogs, logs, or error messages.
- Preserve enough context in messages to identify the operation or file without revealing unnecessary data.

## Settings, Files, and Resources

- Access persisted preferences through `Settings` and keep default behavior cross-platform.
- Validate filesystem targets before writing, and consider collisions, partial files, path traversal, symlinks, disk capacity, and cleanup.
- Add user-visible text through Qt translation mechanisms and update `LANDrop/locales/LANDrop.zh_CN.ts` when appropriate.
- Keep icons and locale files registered in their `.qrc` manifests.

## Dependencies and Platforms

- Add build dependencies in the qmake project and update Linux, macOS, and Windows packaging paths together.
- Do not assume Homebrew's `/opt/homebrew` or `/usr/local` paths in application sources. The harness uses `pkg-config` hints for local libsodium discovery.
- Keep OS-specific code narrowly scoped and validate on every affected platform or report the gap.

## Review Standard

A change is complete when its behavior is clear, ownership and failure paths are safe, relevant wrappers pass, user-visible paths have appropriate manual validation, documentation matches the code, and remaining platform or compatibility gaps are reported.
