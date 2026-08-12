# WireHop Wire Protocol

Version: **1** (negotiated in-session). Version 0 is the implicit LANDrop 0.4.0 wire format; WireHop remains bidirectionally compatible with it. This document is the authority for on-wire behavior; treat any code change that affects it as security-sensitive (see `docs/SECURITY.md`).

## Discovery (UDP port 52637)

JSON datagrams, broadcast to 255.255.255.255 and every interface broadcast address. Datagrams over 4096 bytes are dropped.

Request: `{"request": true}` — every reachable peer replies to the source address with an advertisement.

Advertisement:

```json
{"request": false, "device_name": "...", "device_type": "...", "port": 52638,
 "protocol_version": 1, "caps": ["ack"]}
```

`port` 0 means "not discoverable". `protocol_version` and `caps` are **untrusted hints** (discovery is unauthenticated UDP); they must never gate security or behavior decisions — the in-session values below are authoritative. Receivers must ignore unknown keys.

## Transfer session (one TCP connection)

1. **Key exchange.** Each side immediately sends its raw 32-byte X25519 public key, with no length prefix or preamble of any kind — a peer reads exactly 32 bytes, so nothing may precede them. Shared session key = `crypto_scalarmult(secret, peer_public)`, used directly as the 32-byte AEAD key with no KDF applied.

   **Session code** (six digits, shown to both users for out-of-band comparison), derived as follows. Every step is normative; an implementation that differs displays a different code and breaks the security check:

   1. `digest = BLAKE2b(session_key)` with a **16-byte** output length and no key (libsodium `crypto_generichash` at `crypto_generichash_BYTES_MIN`).
   2. Take the **first 8 bytes** of that digest and read them as a **little-endian** `uint64`.
   3. `code = value mod 1000000`, rendered as decimal and **zero-padded to 6 digits**.
2. **Framing.** Every subsequent message is `2-byte big-endian ciphertext length` + `ciphertext`, where ciphertext = 12-byte random nonce ‖ ChaCha20-Poly1305-IETF(payload). Maximum frame is 65,535 bytes including the 28-byte crypto overhead; the sender chunks file data at 64,000 payload bytes.
3. **Metadata** (sender → receiver, first frame):

   ```json
   {"device_name": "...", "device_type": "...",
    "files": [{"filename": "a.txt", "size": 123}, ...],
    "protocol_version": 1, "caps": ["ack"]}
   ```

   **Receiver validation of metadata** (normative — every field crosses a trust boundary; an implementation that skips these is unsafe, not merely lenient):

   | Rule | Bound |
   | --- | --- |
   | File count | 1–1024 |
   | `filename` length | ≤ 255 UTF-8 bytes |
   | `device_name` length | 1–255 UTF-8 bytes |
   | `size` per file | integer, finite, 0 – 1 TiB (`1099511627776`) |
   | Sum of `size` | ≤ 4 TiB |

   `filename` is a **bare filename, never a path**. Reject: empty, `.`, `..`, anything containing `/` or `\`, absolute paths, drive prefixes, a trailing `.` or space, the characters `<>:"|?*`, Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, with or without an extension), and any string containing Unicode control or bidirectional-override characters. `device_name` is rejected on the same Unicode-control rule. A non-integer, negative, non-finite, or out-of-range `size` is a protocol error, not a clamp.

   Files are written to a temporary file **inside the destination directory** and committed by rename, so a partial or aborted transfer never leaves a file at the final name, and an existing file is never overwritten — a collision gets a numbered variant.

4. **Response** (receiver → sender): `{"response": 0|1, "protocol_version": 1, "caps": ["ack"]}`. Any value other than `1` — including a missing or non-numeric field — is treated as a rejection.
5. **File data.** Raw bytes of each file in metadata order, chunked into frames. There is no per-file header or delimiter: boundaries are implied entirely by the `size` values declared in the metadata, so both sides track a remaining-byte count per file.

   - A sender fills each frame from **exactly one file** (`min(64000, remaining)`), so its frames never straddle a boundary. A receiver must not rely on that: it consumes frames against the declared counts and must handle a frame whose bytes complete one file and begin the next.
   - A file declared `"size": 0` produces **no data frames at all**. It is created and committed when its turn is reached.
   - Receiving more bytes than the metadata declared in total is a protocol error and aborts the session.
6. **Completion acknowledgment** (receiver → sender, capability `ack`): one best-effort `{"ack": 1}` frame after the last file is committed, then disconnect.

## Version and capability negotiation

- `protocol_version` (positive integer) and `caps` (array of strings) ride additively inside the existing metadata and response frames. LANDrop 0.4.0 parses fixed keys and ignores them.
- Absent or malformed fields ⇒ the peer is **version 0 with no capabilities**; the session proceeds with legacy semantics. Negotiation can downgrade but never abort a session.
- The two fields degrade **independently**: a malformed `protocol_version` does not discard a well-formed `caps` list, and vice versa. A peer sending `{"protocol_version": 0, "caps": ["ack"]}` is therefore version 0 *with* the `ack` capability. Capabilities — not the version number — are what gate behavior, so this is the intended reading; the version exists only to signal message-format breaks.
- Bounds on the untrusted list: at most 32 capabilities, each a string of 1–32 **UTF-8 bytes** (not UTF-16 code units — same convention as `MAX_FILENAME_BYTES`). Any violation discards the entire list (fail-to-legacy). Enforced in `WireHop/protocol.cpp`.
- The `caps` array is serialized in sorted order so frames are byte-reproducible across processes.
- A capability is "negotiated" only when both the peer advertises it *and* this build implements it (`FileTransferSession::hasNegotiatedCap`), so a peer cannot induce behavior this build does not support by advertising a capability alone.
- A negotiated feature may take effect only once both sides are informed — i.e. after the response frame has been sent/processed.
- **Versions vs. capabilities:** bump `protocol_version` only for changes that break the message format itself (framing, key exchange, mandatory fields); everything orthogonal is a capability. Unknown capabilities must be ignored.

### Canonical JSON

Implementations **must accept any object key order** — key order carries no meaning on the wire. Separately, for the golden vectors to be byte-comparable across implementations, the *canonical* form is defined as: UTF-8, no insignificant whitespace (compact separators), and object keys serialized in **lexicographic order**. Qt's `QJsonDocument::Compact` already emits sorted keys; other implementations must sort explicitly. Only the golden vectors depend on this — a peer that emits another order is still conformant.

### Defined capabilities

| Capability | Meaning |
| --- | --- |
| `ack` | Endpoint-level: "this build implements the completion-acknowledgment extension" in whichever role it plays. As receiver it sends `{"ack":1}` after committing every file; as sender it honors one. A sender grants the full 10 s acknowledgment window only to peers with which `ack` is negotiated; capless peers get a 2 s grace window (their fast acknowledgment or close is still honored) before the qualified "sent, not confirmed" completion. |

The receiver sends its `{"ack":1}` **unconditionally**, not gated on the sender advertising `ack`. This is deliberate: the frame is additive and ignored by legacy senders, whereas gating it would withhold acknowledgments from any sender that does not advertise caps and regress that sender to a qualified-success message. Capabilities gate *behavior changes*; they do not gate additive frames that legacy peers already tolerate.

### Compatibility matrix

| Sender \ Receiver | LANDrop 0.4.0 | WireHop (this release onward) |
| --- | --- | --- |
| LANDrop 0.4.0 | unchanged | unchanged; extra response keys ignored by the sender |
| WireHop (this release onward) | receiver closes on completion ⇒ sender resolves immediately with qualified success; the 2 s grace only elapses for a peer that neither acknowledges nor closes | full negotiation, 10 s window, confirmed completion |

**On the "acks but advertises no caps" peer class:** it does not exist in any released build. The completion ACK and capability negotiation are unreleased and ship together, so no deployed WireHop sends an ACK without also advertising `ack`. The 2 s grace therefore cannot demote a real peer that would otherwise have been confirmed; it only shortens the dead wait for a peer that goes silent without closing. Should an ACK-without-caps build ever be distributed, revisit the grace duration — the grace timer starts when the sender's last bytes reach the kernel, not when the receiver has them.

## Rules for future changes

New features (resume, larger frames, trusted devices, batched small files) must be introduced as capabilities, activated only when both peers advertise them, and documented here plus in an architecture decision record before shipping.
