# Security

## Trust Boundaries

- UDP discovery packets can be sent by any host able to reach port 52637.
- The TCP listener accepts inbound peers on all interfaces.
- Public keys, framed ciphertext, JSON metadata, filenames, file sizes, and file bytes come from an untrusted peer.
- The configured download path and local selected files cross filesystem boundaries.
- The manual update check consumes remote JSON and can open an external website.

## Existing Security Model

Each transfer exchanges ephemeral scalar-multiplication public keys and derives a session key. Payloads use libsodium ChaCha20-Poly1305 IETF authenticated encryption with random nonces. The application displays a six-digit digest of the session key and asks the receiver to confirm that the sender shows the same code before accepting.

Encryption alone does not establish peer identity. The user comparison and explicit receiver decision are security-relevant behavior and must not be weakened without a documented replacement.

## Required Rules

- Check cryptographic return values, key lengths, ciphertext bounds, and authentication failures before using data.
- Bound all buffering, frame counts, metadata counts, file sizes, and aggregate transfer sizes before allocation or disk writes.
- Reduce incoming filenames to safe leaf names or reject separators, traversal components, absolute paths, device names, and invalid platform-specific names.
- Define collision and partial-file behavior; do not silently overwrite an existing file or leave an incomplete file presented as successful.
- Re-check write results, available space where practical, and final byte counts. Abort safely on short or failed writes.
- Validate discovery JSON, update JSON, IP addresses, and ports; rate-limit or otherwise constrain abusive peers where appropriate.
- Never log session keys, raw file contents, or sensitive local paths unnecessarily.
- Keep libsodium and Qt dependency changes reviewable and update all platform packaging inputs.
- Do not commit signing material, tokens, certificates, or other secrets.

## Known Areas Requiring Care

At initialization time, the receiver constructs output paths from peer-provided filenames and trusts peer-declared sizes with limited bounds. Treat changes near `FileTransferReceiver::processReceivedData()` and `createNextFile()` as security-sensitive, and assess the current behavior before any release.

The two-byte encrypted-frame length and 64,000-byte sender chunk are coupled. Cryptographic overhead or chunk-size changes must prove that every framed message remains representable and bounded.

## Required Review and Validation

Cryptography, protocol, discovery exposure, filename/path handling, download writes, and update-check changes require explicit human security review plus adversarial tests. Include malformed, truncated, oversized, replayed/repeated, traversal, collision, disconnect, and authentication-failure cases as applicable.
