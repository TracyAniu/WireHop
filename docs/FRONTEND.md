# Desktop UI

## UI Model

WireHop is a Qt Widgets system-tray application, not a browser frontend. The tray menu is the primary navigation; `.ui` files define the dialogs for file selection, peer selection, transfer progress, settings, and about information.

## Interaction Rules

- Preserve the tray-first workflow and keep primary actions discoverable without a persistent main window.
- Keep send and receive dialogs above ordinary windows where the existing `WindowStaysOnTopHint` pattern applies.
- Maintain clear states for discovery, connecting, handshaking, approval, transferring, completion, rejection, timeout, and error.
- Never imply that a transfer succeeded before the socket/session and filesystem work completes.
- Keep destructive or trust-sensitive actions explicit. Receiving files requires a clear file summary, session code, and Yes/No choice.
- Support both file-picker and drag-and-drop input for regular local files.

## Forms, Strings, and Assets

- Edit the relevant `WireHop/*.ui` form rather than generated headers.
- Add icons under `WireHop/icons/` and register them in `WireHop/icons.qrc`.
- Wrap user-visible C++ strings in `tr()` and update the `.ts` catalog when strings change.
- Check long filenames, large formatted sizes, translated strings, high-DPI rendering, keyboard focus, default buttons, and disabled/loading states.

## Validation

Run `./scripts/typecheck.sh` and `./scripts/smoke.sh`, then exercise the affected native dialog path. UI changes need visual inspection on every affected operating system; when only one platform is available, report the others as unverified.
