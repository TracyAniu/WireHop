# Product

## Purpose

LANDrop is a cross-platform desktop utility for sending files directly between devices on the same local network. This repository contains the C++/Qt desktop application identified as version 0.4.0 in `LANDrop/main.cpp`.

The root README explicitly says this source snapshot does not reflect the latest LANDrop releases. Treat the repository as the authority for work on this codebase, not as documentation of the current hosted product.

## Target Users

- People who want to move photos, videos, or other regular files between nearby devices without routing file data through an Internet service.
- Users working across desktop and mobile platforms on the same LAN or personal hotspot.

## Core Workflows

1. Launch LANDrop and keep it available from the system tray.
2. Select or drag in one or more regular files, discover a peer or enter its address and port, and start a transfer.
3. On the receiving device, review the sender, file summary, total size, and six-digit session code, then accept or reject the transfer.
4. Configure the device name, download directory, discoverability, and listening port.
5. Open the configured download directory or manually check for product updates from the tray UI.

## In Scope

- Local peer discovery over UDP.
- Direct encrypted file transfer over TCP.
- Explicit receiver approval and session-code comparison.
- System-tray UI, transfer progress, settings, and Simplified Chinese localization.
- qmake builds for Linux, macOS, and Windows, with packaging workflows in `.github/workflows/package.yml`.

## Out of Scope

- Cloud storage, relays, user accounts, or Internet-based device discovery.
- File compression or media transcoding.
- Background update installation; the application only checks a remote version document and opens the download website.
- Claims about features in newer closed-source LANDrop releases.

## Product Principles

- Keep the common send/receive path short and understandable.
- Transfer original file bytes directly on the local network.
- Require the receiver to make an explicit trust decision before writing file contents.
- Keep platform-specific behavior behind Qt and the packaging layer where practical.

## Important Terms

| Term | Meaning |
| --- | --- |
| Discovery | UDP broadcast/request exchange on port 52637 used to advertise a device and its TCP transfer port. |
| Discoverable | Setting that controls whether the advertised transfer port is nonzero. |
| Session code | Six-digit digest derived from the negotiated session key and shown on both devices for out-of-band comparison. |
| Transfer session | One TCP connection that performs key exchange, metadata approval, and encrypted file-data transfer. |
