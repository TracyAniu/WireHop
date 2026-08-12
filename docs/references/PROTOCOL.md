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

1. **Key exchange.** Each side immediately sends its raw 32-byte X25519 public key. Shared session key = `crypto_scalarmult(secret, peer_public)`. The six-digit session code shown to both users is derived from a BLAKE2b digest of the session key, mod 10^6, zero-padded.
2. **Framing.** Every subsequent message is `2-byte big-endian ciphertext length` + `ciphertext`, where ciphertext = 12-byte random nonce ‖ ChaCha20-Poly1305-IETF(payload). Maximum frame is 65,535 bytes including the 28-byte crypto overhead; the sender chunks file data at 64,000 payload bytes.
3. **Metadata** (sender → receiver, first frame):

   ```json
   {"device_name": "...", "device_type": "...",
    "files": [{"filename": "a.txt", "size": 123}, ...],
    "protocol_version": 1, "caps": ["ack"]}
   ```

4. **Response** (receiver → sender): `{"response": 0|1, "protocol_version": 1, "caps": ["ack"]}`.
5. **File data.** Raw bytes of each file in metadata order, chunked into frames.
6. **Completion acknowledgment** (receiver → sender, capability `ack`): one best-effort `{"ack": 1}` frame after the last file is committed, then disconnect.

## Version and capability negotiation

- `protocol_version` (positive integer) and `caps` (array of strings) ride additively inside the existing metadata and response frames. LANDrop 0.4.0 parses fixed keys and ignores them.
- Absent or malformed fields ⇒ the peer is **version 0 with no capabilities**; the session proceeds with legacy semantics. Negotiation can downgrade but never abort a session.
- Bounds on the untrusted list: at most 32 capabilities, each a string of 1–32 **UTF-8 bytes** (not UTF-16 code units — same convention as `MAX_FILENAME_BYTES`). Any violation discards the entire list (fail-to-legacy). Enforced in `WireHop/protocol.cpp`.
- The `caps` array is serialized in sorted order so frames are byte-reproducible across processes.
- A capability is "negotiated" only when both the peer advertises it *and* this build implements it (`FileTransferSession::hasNegotiatedCap`), so a peer cannot induce behavior this build does not support by advertising a capability alone.
- A negotiated feature may take effect only once both sides are informed — i.e. after the response frame has been sent/processed.
- **Versions vs. capabilities:** bump `protocol_version` only for changes that break the message format itself (framing, key exchange, mandatory fields); everything orthogonal is a capability. Unknown capabilities must be ignored.

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
