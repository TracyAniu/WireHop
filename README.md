# WireHop

<img src="WireHop/icons/banner.png" width="460" alt="WireHop — Files, one hop away.">

[![Package](https://github.com/TracyAniu/WireHop/actions/workflows/package.yml/badge.svg)](https://github.com/TracyAniu/WireHop/actions/workflows/package.yml)

WireHop is a cross-platform desktop utility for sending files directly between devices on the same local network. It uses direct encrypted connections, requires receiver approval, and does not route file contents through a cloud service.

WireHop 0.1.0 is an independently maintained fork derived from the open-source LANDrop 0.4.0 snapshot. It has its own name, application identifiers, release channel, and original artwork. The transfer wire protocol is intentionally unchanged for compatibility with that snapshot.

## Features

- Direct local-network file transfer over TCP.
- UDP peer discovery plus manual address entry.
- Authenticated encryption using libsodium.
- Receiver approval and a six-digit session code for peer comparison.
- Collision-safe receiving that does not overwrite existing files.
- System-tray workflow on Linux, macOS, and Windows.
- Simplified Chinese localization.

## Build

WireHop requires Qt 5, a C++11 toolchain, qmake, make, and libsodium development files.

```sh
git clone git@github.com:TracyAniu/WireHop.git
cd WireHop
./scripts/typecheck.sh
```

Run it locally with:

```sh
./scripts/dev.sh
```

On a Debian-based Linux system, install libsodium with `sudo apt install libsodium-dev`. You can also build directly with qmake:

```sh
mkdir build
cd build
qmake ../WireHop/WireHop.pro
make -j2
```

## Validation

```sh
./scripts/lint.sh
./scripts/test.sh
./scripts/typecheck.sh
./scripts/smoke.sh
```

See `docs/TESTING.md` for manual peer-transfer and platform checks.

## License and attribution

The source code and original WireHop artwork are distributed under the BSD 3-Clause License in `LICENSE`. WireHop retains the required LANDrop copyright and license notice because it is a derivative of LANDrop 0.4.0. See `THIRD_PARTY_NOTICES.md` for Qt, libsodium, retained Material Design Icons, and upstream attribution.

Before publishing binaries, include the license texts and notices required by the exact Qt and other dependency packages bundled in those binaries. Signing, notarization, and store-specific requirements are separate release tasks.
