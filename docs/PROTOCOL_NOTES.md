# KDE Connect protocol v8 - implementation notes for Tatar

Purpose: let a Rust engineer implement a protocol-8 KDE Connect client that the **unmodified**
official Android (and, where noted, iOS) app accepts, without browsing the reference code.

Everything below was read from the reference implementations on 2026-10-28 and is described in
our own words (no code copied; the references are GPL). Field names, constants, byte formats and
orderings are exact.

## 0. Sources and how to read citations

| Short name | Repo / ref read | Notes |
|---|---|---|
| **KDE** | `invent.kde.org/network/kdeconnect-kde`, `master` @ `c624975b` (latest tags seen: v26.08.x) | Desktop (C++/Qt). Paths relative to repo root. |
| **AND** | `invent.kde.org/network/kdeconnect-android`, `master` @ `b2a2b035` (latest tag seen: v1.37.0) | Phone app (Kotlin). Paths under `src/main/java/org/kde/kdeconnect/`. |
| **IOS** | `invent.kde.org/network/kdeconnect-ios`, `master` | Only skimmed for TLS roles, capabilities, battery request. |

"Phone" below means the official Android app unless stated. Where the two reference apps differ
the difference is called out. Items marked **[UNVERIFIED]** are in section 11.

Recent changes that matter (all already in the sources above):
- Protocol 8 handshake (post-TLS identity re-exchange, deviceId/protocolVersion consistency checks) - AND `backends/lan/LanLinkProvider.kt`, KDE `core/backends/lan/lanlinkprovider.cpp` (`encrypted()`).
- Pairing timestamp + verification key including it (v8) - AND `PairingHandler.kt`, KDE `core/backends/pairinghandler.cpp`.
- `targetDeviceId` / `targetProtocolVersion` in the plain TCP identity - both LanLinkProviders.
- KDE commit "Do not auto-accept pairings, just do not unpair on redundant confirmations" (2026-10-08) - changes how a repeated `pair:true` is treated when already paired (see 5.4).
- AND `NetworkPacket.kt` is now kotlinx-serialization based; reading is lenient about number/bool types (see Gotchas).

---

## 1. Identity packet (`kdeconnect.identity`)

### 1.1 Wire framing (applies to every packet)
- One packet = one UTF-8 JSON object serialized **compactly** (no embedded newlines) followed by a single `\n` (0x0A). Receivers split on `\n`.
  (KDE `core/networkpacket.cpp` `serialize()`; AND `NetworkPacket.kt` `serialize()`.)
- Top-level keys:
  - `id`: JSON **number**, milliseconds since the Unix epoch at send time (both references write `currentMSecsSinceEpoch`). Receivers ignore it. Send a number, not a string.
  - `type`: string, e.g. `"kdeconnect.identity"`. Packets with an empty/missing `type` are discarded (KDE `unserialize`).
  - `body`: JSON object (may be `{}`).
  - `payloadSize` (integer, may be `-1`) and `payloadTransferInfo` (object): present **only** when the packet announces a payload. Android omits both keys otherwise (it does not write nulls); KDE also omits them.
- Size limits actually enforced by the references:
  | Context | KDE | AND |
  |---|---|---|
  | plain TCP identity line (pre-TLS) | 8192 bytes | 512 KiB (`MAX_IDENTITY_PACKET_SIZE`) |
  | post-TLS identity line | 8192 bytes | 512 KiB |
  | UDP datagram | (read whole datagram) | buffer 512 KiB |
  | normal packet line after pairing | 32 MiB (`MAX_PACKET_SIZE`) | 32 MiB |
  Tatar: enforce 8192 for identity lines it reads (stricter is fine; a real Android identity with ~20 capabilities is a few hundred bytes), 32 MiB for later lines, and drop/close on overflow.
  (KDE `core/backends/lan/lanlinkprovider.cpp`, `landevicelink.cpp`; AND `backends/lan/LanLinkProvider.kt`, `LanLink.kt`.)

### 1.2 Full identity body (sent inside TLS, and by Android in UDP/TCP too)
Built by AND `DeviceInfo.toIdentityPacket()` and KDE `core/deviceinfo.h` `toIdentityPacket()`:

| Field | Type | Meaning / rules |
|---|---|---|
| `deviceId` | string | Must match `^[a-zA-Z0-9_-]{32,38}$` (see 1.3). |
| `deviceName` | string | User-visible name. Must be non-blank **after** filtering (1.4) or the packet is rejected. |
| `deviceType` | string | One of `desktop`, `laptop`, `phone`, `tablet`, `tv`. Unknown values: AND maps to desktop; KDE maps to "unknown". Use `laptop` for MacBooks, `desktop` otherwise. |
| `protocolVersion` | integer | `8`. |
| `incomingCapabilities` | array of strings | Packet types this device can **receive**. Unique strings. |
| `outgoingCapabilities` | array of strings | Packet types this device can **send**. |
| `tcpPort` | integer | **Only in the UDP broadcast variant** (1.5). Not in the in-TLS identity. |
| `targetDeviceId`, `targetProtocolVersion` | string, integer | **Only in the plain TCP identity** (1.6). |

Android also sends `tcpPort` in UDP only; neither app sends other identity fields. Unknown body keys are ignored by both.

### 1.3 deviceId validation
- Regex in both apps (KDE `core/deviceinfo.h` `isValidDeviceId`; AND `DeviceInfo.kt` `DEVICE_ID_REGEX`): `^[a-zA-Z0-9_-]{32,38}$`. A packet with an invalid deviceId is discarded at every stage (UDP, plain TCP, in-TLS).
- Generation: KDE uses a random UUID rendered as 32 lowercase hex chars (no dashes) and writes it as the certificate CN (`core/kdeconnectconfig.cpp` `generateCertificate`). AND now does the same: UUID with dashes removed (`helpers/DeviceHelper.kt` `initializeDeviceId`). Older installs have 36-char UUIDs with underscores (the AND test certificates use e.g. `ee061a75_e403_4ecc_9261_5ffe2722f698`), so peers' ids may contain `_` and `-` and be up to 38 chars.
- KDE additionally rewrites every non-`[A-Za-z0-9_]` character of any received `deviceId` to `_` (for D-Bus safety, `core/networkpacket.cpp` `unserialize` + `dbushelper.cpp`). A `-` in a deviceId therefore becomes `_` inside KDE and the cert-CN comparison is done on the filtered CN too (`addLink` filters the cert CN the same way). Tatar should generate ids that are exactly 32 lowercase hex chars (no `-`/`_`) so every peer treats them identically.
- The deviceId **is** the certificate CN of the device's cert (see section 4) and is checked against the peer cert after the handshake (see 3.5).

### 1.4 deviceName sanitising
Both apps apply the same filter to a name received from the network (KDE `core/deviceinfo.h` `filterName`; AND `helpers/DeviceHelper.kt` `filterInvalidCharactersFromDeviceNameAndLimitLength`):
1. Remove every one of these characters: `" ' , ; : . ! ? ( ) [ ] < >`
2. (AND only) trim whitespace.
3. Truncate to 32 characters (Unicode scalar/UTF-16 unit as the platform counts; keep names short and ASCII-safe to be sure).
Validity rule: filtered name must not be empty/blank. Apply the same filter to the name we advertise and to names we display.

### 1.5 UDP discovery variant
KDE sends only: `deviceId`, `deviceName`, `protocolVersion`, `tcpPort` (`toUdpDiscoveryPacket`). Android sends the **full** identity plus `tcpPort`. Android's receive check for UDP is the generic identity validity (type identity, non-blank filtered `deviceName`, valid `deviceId`) plus `tcpPort` in 1716..1764 (AND `unserializeReceivedIdentityPacket`, `udpPacketReceived`). KDE's check requires only valid `deviceId`, then `protocolVersion >= 8` and port in range.
Recommended for Tatar: send the KDE minimal form plus `deviceType` (keeps the datagram small, no fragmentation; `deviceName` is mandatory for Android).

### 1.6 Plain TCP identity variant (first packet on a new TCP connection)
Sent only by the side that **opened** the TCP connection. Body: `deviceId`, `deviceName`, `protocolVersion`, `targetDeviceId` (the id of the peer we are connecting to), `targetProtocolVersion` (the `protocolVersion` we saw in the peer's UDP/mDNS announcement). Android sends the full identity (with capabilities) plus these two; KDE sends the minimal five fields (KDE `toConnectionPacket`).
- **KDE master requires `targetDeviceId` to be present and a valid deviceId** (its `isValidConnectionPacket` calls the id validator on it, so a missing one fails the check), even though the later code treats an empty one as "don't care". Android treats both target fields as optional. Therefore Tatar must always send both.
- If present, `targetDeviceId` must equal the receiver's own id and `targetProtocolVersion` must equal 8 or the receiver closes the connection (both apps).
- Quirk: Android writes `targetProtocolVersion` as a JSON **string** (`"8"`) in its connection identity (it copies the peer's value via `getString`). Parse leniently (accept int or numeric string) when reading; always write an integer.

### 1.7 Capabilities
See section 8. Android also treats a **missing/empty** capability set as "unknown, assume everything supported" (`Device.supportsPacketType` falls back to `true` when the set is null) but Tatar must always send both arrays.

---

## 2. Discovery

### 2.1 Constants
- UDP port: **1716** (both for sending and for receiving). TCP listen range: **1716-1764** inclusive; each device binds the first free port starting from 1716 and advertises it. Payload (file transfer) listen ports start at **1739** up to 1764. (KDE `lanlinkprovider.h`: UDP_PORT/MIN_TCP_PORT/MAX_TCP_PORT; AND `LanLinkProvider.kt` companion: UDP_PORT, MIN_PORT, MAX_PORT, PAYLOAD_TRANSFER_MIN_PORT.)
- Tatar must **bind UDP 1716 with address reuse** (so it can coexist with another KDE Connect on the same machine) listening on all IPv4 interfaces, and bind a TCP listener in 1716..1764.

### 2.2 UDP broadcast
- Who: both sides broadcast an identity datagram to `255.255.255.255:1716`, KDE once per usable IPv4 interface address (it binds a send socket to each interface's address in turn, skipping loopback) so the datagram leaves on every network; plus unicast datagrams to each configured "custom device" IP (manually added by the user).
- When: on start, and on every network change (KDE debounces to one event loop iteration via a 0 ms single-shot timer; AND ignores network-change triggers within 200 ms of the last one, `DELAY_BETWEEN_BROADCASTS`). There is **no periodic rebroadcast** in either app; discovery is event-driven (start + network change + reverse-connect, below).
- Ignore: datagrams from loopback (unless test mode), our own `deviceId`, invalid identity, `protocolVersion < 8`, `tcpPort` outside 1716-1764.
- **What the receiver does:** the receiver of a UDP identity connects **TCP to the sender's source IP** (IPv6-mapped IPv4 is converted to IPv4) on the `tcpPort` from the body, and sends the plain TCP identity (1.6) first. I.e. *whoever hears the broadcast opens the TCP connection to the broadcaster.* (KDE `udpBroadcastReceived` -> `deviceDiscovered` -> `tcpSocketConnected`; AND `udpPacketReceived`.)
- Reverse connect: if KDE's outgoing TCP connection fails or the write fails, it unicasts its own identity datagram to the peer (to UDP 1716), asking the peer to connect to it instead (`recoverConnectionError`). Android does the same trick on UDP receive errors by rebroadcasting. Tatar should do the same.
- Rate limiting (apply on receive):
  | | KDE | AND |
  |---|---|---|
  | minimum gap between accepted packets/connections from same source IP | 500 ms | 1000 ms |
  | same, keyed by deviceId | 500 ms | 1000 ms |
  | tracked-entry cap | 1024 | 255 |
  | max simultaneously visible **unpaired** devices | 42 | 42 |
  Tatar: apply 500 ms by IP and by deviceId; cap at 42 unpaired links.
- Android only accepts UDP/TCP from "private" source addresses: loopback, site-local (RFC1918), link-local, CGNAT (100.64/10) and unique-local IPv6, or an address in its custom-device list (AND `helpers/NetworkHelper.kt` `isPrivateAddress`). Tatar's host must be on such an address from the phone's point of view.
- Android also has a "trusted networks" setting: on a non-trusted Wi-Fi it neither broadcasts, announces mDNS, nor reacts to unpaired devices; paired devices are still accepted. Not something Tatar controls (document in the troubleshooting screen).

### 2.3 mDNS / DNS-SD
- Service type: `_kdeconnect._udp` (domain `.local`). Android uses `NsdManager` with that type; KDE uses `_kdeconnect._udp.local` (KDE `core/backends/lan/mdnshdiscovery.cpp`; AND `backends/lan/MdnsDiscovery.kt`).
- **Instance (service) name = the deviceId.** The peer treats the instance name as the device id before resolving anything.
- SRV port = the advertiser's TCP listen port (1716-1764).
- TXT records (each key+value < 255 bytes): `id` (deviceId), `name` (device name), `type` (deviceType; Android writes it, KDE master omits it), `protocol` (string of the protocol version, e.g. `8`). Only `protocol` is read by KDE. Android reads **none** of them.
- **How a device found by mDNS is contacted - the two apps differ:**
  - KDE: on `serviceFound` it reads `protocol` from TXT and connects TCP straight to the resolved address and SRV port (`deviceDiscovered(address, port, name(=deviceId), protocol)`), sending the plain TCP identity. So **Tatar discovering a phone via mDNS can connect directly** to host:SRV-port, using the instance name as the deviceId and the TXT `protocol` (reject if < 8 or missing/zero).
  - Android: after resolving a service it does **not** connect to the SRV port. It sends its own UDP identity datagram **to the resolved host on UDP 1716**, which makes the *other side* connect to Android's TCP port. Consequently, for a phone to find Tatar via mDNS, Tatar must listen on UDP 1716 and react to that datagram exactly like a broadcast (connect TCP to the phone's `tcpPort`). Android skips services whose instance name equals its own id or whose id it already has a link for.
- Both apps stop/restart discovery on network change (KDE `onNetworkChange`: re-announce + restart discoverer). Android announces only on trusted networks.
- Recommendation for Tatar: implement UDP + mDNS (`mdns-sd` crate): announce with instance name = deviceId and TXT `id`, `name`, `type`, `protocol=8`; discover and, for each result not already linked and not ourselves, connect directly as KDE does. Always keep the UDP 1716 listener (it is what makes Android's mDNS path work, and it also serves "add by IP" flows).

### 2.4 Manual add-by-IP
Mirror KDE `getBroadcastAddresses`: send the UDP identity datagram directly to the typed IP at UDP 1716 (phone then connects to us), and also attempt a direct TCP connect to ip:1716 (phone's first TCP port) with the plain identity. On Android the user may also have to add the desktop's IP in the phone's "Add device by IP" so that the phone's private-address filter and its own unicast work; the phone accepts packets from any RFC1918 address anyway.

---

## 3. Connection sequence

### 3.1 Roles (the "inverted" TLS roles) - verified in both apps and in iOS
> **The side that opened the TCP connection acts as the TLS *server*; the side that accepted the TCP connection acts as the TLS *client*.**

- KDE: after the connecting side writes its plain identity (`tcpSocketConnected`) it calls `startServerEncryption()`; the accepting side reads the identity (`tcpPacketReceived`) and calls `startClientEncryption()`. (`core/backends/lan/lanlinkprovider.cpp`)
- AND: `clientMode = (connectionStarted == Locally)` where "Locally" = we accepted the TCP connection; so TCP-acceptor = TLS client, TCP-initiator = TLS server. (`LanLinkProvider.kt` `identityPacketReceived`)
- IOS: connecting side `kCFStreamSSLIsServer = YES` with always-authenticate client auth; accepting side `IsServer = 0` (`ObjC Network Backend/lanBackend/LanLinkProvider.m`).

Sequence (A = initiator/TCP client = TLS server, B = acceptor/TCP server = TLS client):
1. A opens TCP to B (B's port from UDP `tcpPort` or mDNS SRV).
2. A writes its **plain** identity line (1.6) and immediately starts the TLS handshake as **server**. B does **not** send a plain identity.
3. B reads exactly one line (must arrive within 1 s on KDE; size <= 8192 on KDE/512 KiB on AND), validates it (valid identity, `targetDeviceId`/`targetProtocolVersion` match us, `protocolVersion >= 8`, not a downgrade for a trusted device), then starts TLS as **client**.
4. TLS handshake (3.2-3.4).
5. **Protocol 8 secure identity exchange:** as soon as the handshake completes, **both** sides write their full identity line (1.2, no `tcpPort`, no `target*`) and read the peer's. There is no strict "first sender": both send immediately and concurrently (AND writes then reads; KDE writes in the same callback that arms the reader). The reader must wait for the peer's line; KDE gives up after 1000 ms, AND uses a 10 s socket timeout. Tatar: write immediately, read with a >= 5 s timeout, and do not wait for the peer before writing.
6. Checks on the secure identity (3.5). On success the link is established and subsequent lines are ordinary packets. Then pairing packets or plugin traffic (paired only) follow.

### 3.2 TLS parameters
- Version: **Android caps its context at TLS 1.2** (`SSLContext.getInstance("TLSv1.2")`, with the comment that 1.3 caused issues on some devices) - AND `helpers/security/SslHelper.kt`. KDE (Qt) negotiates up to 1.3 between desktops. Tatar must support at least TLS 1.2; allow 1.3 too for desktop peers.
- Cipher suites: no explicit list in either app; whatever Conscrypt/Qt default to. Android 1.2 defaults are ECDHE with AES-GCM/ChaCha20 (ECDSA or RSA). Any standard rustls TLS 1.2 ECDHE suite works; avoid depending on static-RSA key exchange.
- Server name / hostname: not used for trust. KDE sets a "peer verify name" = deviceId but disables any CA path; Android does not enable endpoint identification. With rustls pass a fixed dummy `ServerName` (deviceIds with `_` are not valid DNS names) and use a custom verifier.
- **Client certificates are requested by the TLS server on every connection** and must be sent by the TLS client:
  - AND as TLS server: `needClientAuth = true` if the peer's deviceId is a trusted (paired) device, else `wantClientAuth = true` (optional).
  - KDE as TLS server: `VerifyPeer` (mandatory, cert must equal stored) when trusted, else `QueryPeer` (request, accept anything).
  - IOS: always authenticates.
  So Tatar as TLS server: request a client cert always; mandatory when the peer is paired, optional (but still then pin/inspect what comes) when unpaired. Tatar as TLS client: always present our cert.

### 3.3 Certificate validation
- **Unpaired peer:** accept any certificate, including self-signed (Android installs a trust-all manager; KDE ignores only the "self-signed" error and treats every other error as fatal, `sslErrors()`). The cert is only recorded, to be pinned on pairing.
- **Paired peer:** the stored certificate is the *only* trust anchor. The presented leaf must be **byte-identical** to the stored one (Android puts the stored cert in the trust store and PKIX-validates the self-signed leaf against it; KDE sets `caCertificates={storedCert}` with `VerifyPeer`). Tatar: compare DER bytes (or SHA-256 of DER) and reject on mismatch **without** falling back to "unpaired" behaviour.
- Which cert to store: the peer's end-entity certificate captured from the TLS session at pairing time (AND `device.certificate`, saved by `DeviceInfo.saveInSettings`; KDE `addTrustedDevice`). Store the full DER.
- Handshake signature verification still must work with the peer's key type (EC P-256 today; old Android installs can still have RSA-2048). Do not validate dates, chain, or hostname for these self-signed certs for anything except the pinned-equality rule.

### 3.4 Pre-TLS downgrade protection
For a trusted device, a received `protocolVersion` lower than the last stored one is refused (KDE `isProtocolDowngrade`; AND same). Tatar must persist each paired device's last-seen protocolVersion and apply this.

### 3.5 Post-TLS identity checks (CVE-2025-66270 mitigation) - all mandatory
After parsing the in-TLS identity line (both apps do these in order):
1. Valid identity packet (type identity, filtered `deviceName` non-blank, valid `deviceId`), else abort.
2. `protocolVersion` equals the value from the pre-TLS stage (the UDP/mDNS/plain-TCP one), else abort.
3. `deviceId` equals the id from the pre-TLS stage (the one we used to decide "trusted or not" and to pick the pinned certificate), else abort.
4. (`addLink`) the **peer certificate's subject CN, after replacing every char outside `[A-Za-z0-9_]` with `_` for KDE's comparison**, must equal the identity's `deviceId` (KDE filters both). Else abort. The CN is read from the TLS peer's end-entity cert. In the AND code read, no separate CN-vs-deviceId comparison was found in `LanLinkProvider.kt`; for paired devices its tie to the cert is the pinned trust store. Tatar must perform check 4 in both paired and unpaired cases.
5. If a link for this deviceId already exists: the new peer cert must equal the existing link's cert, otherwise close the **new** socket (KDE `addLink`, AND `addOrUpdateLink`).
Only after all pass may any other packet type be processed. Capabilities are taken from the **secure** identity, never from the plain one.

### 3.6 Two simultaneous connections (race)
- If both devices connect at once, two links to the same deviceId can complete. Both apps keep one link object per deviceId: a second successful handshake with the **same certificate replaces** the socket of the existing link (KDE `LanDeviceLink::reset`, AND `LanLink.reset`: swap socket, close the old one, old reader's disconnect is ignored if a newer socket exists).
- Mitigations in the references: per-IP and per-deviceId rate limits (500 ms / 1000 ms) drop the second attempt that arrives immediately after the first; Android additionally ignores mDNS results for ids it already has a link to.
- Inference (not tested): in a perfectly symmetric race each side could swap to the socket the other side then closes, dropping the link; both apps recover on the next discovery event. Tatar: (a) apply the same rate limits; (b) do not open an outgoing connection to a deviceId that already has a live link or an in-progress handshake; (c) when a replacement socket for the same cert arrives, swap and close the old one, and treat the loss of the old socket as non-fatal if a newer one exists (wait ~300 ms before declaring disconnect like AND).

---

## 4. Certificate

What both current apps generate (KDE `core/sslhelper.cpp`, `core/kdeconnectconfig.cpp`; AND `helpers/security/RsaHelper.kt`, `SslHelper.kt`):

| Item | Value |
|---|---|
| Key | **EC, curve secp256r1 / prime256v1 (P-256)**. (KDE can still *load* an RSA-2048 key from older configs - key algorithm is stored in its config; AND generates EC in current versions, RSA only for legacy installs. Tatar: generate EC P-256.) |
| X.509 version | v3 (no extensions in the cert AND builds; KDE sets none either) |
| Serial | KDE: random 160-bit; AND: fixed `1`. Anything <= 20 bytes is fine. |
| Subject = Issuer | `CN=<deviceId>`, `OU=KDE Connect`, `O=KDE` (RDN order is not significant; KDE emits CN,O,OU, AND emits CN,OU,O). Self-signed. |
| notBefore | now minus 1 year (AND: start of today minus 1 year) |
| notAfter | now plus 10 years |
| Signature algorithm | **ecdsa-with-SHA512** for EC keys (SHA512withRSA for RSA). Confirmed on the AND unit-test certificate (`ecdsa-with-SHA512`, serial 1, subject `CN=ee061a75_..., OU=KDE Connect, O=KDE`, validity 2023-09-15 to 2034-09-15). |
Tatar with `rcgen`: P-256 key, `PKCS_ECDSA_P256_SHA256` is the default - to match the references use SHA-512 signing if `rcgen` can (`ring` cannot sign ECDSA-P256 with SHA-512; then SHA-256 is acceptable - **[UNVERIFIED]** that Android accepts a cert signed with ecdsa-with-SHA256; nothing in the Android code inspects the cert's signature algorithm beyond the JVM's default algorithm constraints, and SHA-256 is allowed there. Test against a real phone.)
- Persist: private key in OS keychain (CLAUDE.md rule); the certificate (public) may be stored as a file/in the same keychain item. The deviceId equals the CN, so **regenerating the cert changes the identity**; generate once and reuse. Android regenerates its own cert (and wipes all trusted devices!) if the stored one is expired, not-yet-valid, or its CN differs from the device id (`SslHelper.initialiseCertificate`) - so never change the CN while keeping the id.
- Validity check on load (KDE): not yet effective or expired -> regenerate.

---

## 5. Pairing (`kdeconnect.pair`)

Pairing happens over an already established, encrypted link (section 3). Unpaired devices may exchange **only** `kdeconnect.identity` and `kdeconnect.pair`; both apps ignore other packets from unpaired peers - and treat any other packet from an unpaired peer as "the peer thinks we are paired" and **answer by sending `pair:false` / unpairing** (KDE `Device::privateReceivedPacket` else-branch calls `unpair()`; AND `Device.onPacketReceived` calls `unpair()`). Tatar must therefore never send plugin packets before pairing completes, and on a link where Tatar considers itself paired but the phone does not, Tatar will be unpaired by the phone's `pair:false`; handle that by dropping the stored trust (and showing UI).

### 5.1 Packet bodies
| Packet | Body |
|---|---|
| Pair request | `{"pair": true, "timestamp": <integer unix seconds>}` |
| Pair accept (response to a request) | `{"pair": true}` (no timestamp) |
| Reject / cancel / unpair | `{"pair": false}` |
- `timestamp`: whole **seconds** since the Unix epoch (not ms), JSON integer, taken from the sender's clock at request time. A request without a timestamp, or with `|timestamp - now| > 1800` s (30 min) is not accepted (KDE: "Device clocks are out of sync" error; AND: sets NotPaired and reports clocks not matching; IOS: same constant). Android also rejects missing timestamps (-1) by unpairing.
- No other fields.

### 5.2 State machine (identical in KDE `core/backends/pairinghandler.cpp`, AND `PairingHandler.kt`)
States: `NotPaired`, `Requested` (we sent the request), `RequestedByPeer`, `Paired`.
- Outgoing: user starts pairing -> state `Requested`, store `pairingTimestamp = now_secs`, send `{pair:true,timestamp}`, start a **30 s** timer; on timeout send `{pair:false}` and go `NotPaired` ("timed out").
- Incoming `pair:true`:
  - in `Requested` -> peer accepted: cancel timer, **Paired** (store peer cert).
  - in `NotPaired` -> must carry a valid timestamp (5.1): store it as `pairingTimestamp`, go `RequestedByPeer`, start a timer (KDE 30 s; AND 25 s, "time to show notification"), show UI with the verification key. User accepts -> send `{pair:true}` then **Paired**; user rejects -> send `{pair:false}`, `NotPaired`.
  - in `RequestedByPeer` -> ignore (duplicate before timeout).
  - in `Paired`: KDE (since 2026-10-08) ignores it if there is no `timestamp` (a redundant accept), otherwise unpairs and treats it as a new request; AND (current master) always unpairs and treats it as a new request, which then fails if there is no timestamp (it ends in NotPaired + "unpaired" callback). **Tatar must never send a redundant `pair:true` while paired**, and must treat `pair:true` + `timestamp` in `Paired` as "peer lost its trust": unpair locally and process as a new incoming request.
- Incoming `pair:false`: cancel timer; in `Requested`/`RequestedByPeer` -> `NotPaired` (failed: "canceled by other peer"); in `Paired` -> `NotPaired`, forget the peer's cert and settings; in `NotPaired` -> ignore.
- Simultaneous requests: if we are `RequestedByPeer` and the user presses "pair" again, accept the peer's request (both apps do that); if we are `Requested` and receive a request from the peer, per the table above it counts as an accept. (Note this means two near-simultaneous requests both end `Paired`; this is the references' behaviour.)
- On **Paired** the peer's certificate (captured from the TLS session) is persisted together with name, type, protocolVersion. From then on that cert is pinned (3.3). Plugins are loaded only for paired + reachable devices.
- Unpair by user: send `{pair:false}` (if reachable), forget cert. Link may stay up; re-pair needs a new request.

### 5.3 Verification key (protocol 8) - exact algorithm
Defined by AND `PairingHandler.getVerificationKey` and KDE `PairingHandler::verificationKey` (they agree byte for byte); test vectors are in AND `src/test/java/org/kde/kdeconnect/PairingHandlerTest.kt` and were **re-derived independently with OpenSSL + SHA-256 for this document** (section 5.5).

Inputs: our certificate, the peer's certificate (the one seen in the TLS handshake of this link), and `timestamp` = the `timestamp` value of the pair *request* (ours if we sent it, the peer's if we received it) as a decimal integer.
1. For each certificate take the **SubjectPublicKeyInfo DER** of its public key (the standard X.509 `SubjectPublicKeyInfo`, ASN.1 DER, as `publicKey().toDer()` in Qt / `publicKey.encoded` in Java). For an EC P-256 key this is exactly 91 bytes (`30 59 30 13 06 07 2a8648ce3d0201 06 08 2a8648ce3d030107 03 42 00 04 || X(32) || Y(32)`). **Not** the whole certificate, **not** the raw EC point.
2. Compare the two byte strings as unsigned bytes, lexicographically (shorter-prefix is smaller). Put the **larger** first: `data = larger || smaller`. If equal, order is irrelevant.
3. Append the ASCII decimal representation of the timestamp (no sign, no padding, no separator): `data = larger || smaller || ascii(timestamp)`.
4. `digest = SHA-256(data)` (32 bytes).
5. Display string = first **8 hexadecimal characters** of the lowercase hex of the digest, **upper-cased** (e.g. `54DC916E`) = the first 4 bytes. Both apps show exactly this 8-char string (this is the lengthened code from the CVE-2025-32898 fix; protocol 7 used the same construction without the timestamp). Format for display: Android shows the plain 8 chars; group in 4+4 only as a presentation choice - compare ignoring any spacing.
Protocol-7-style key (no timestamp) is only used by the references when the peer's `protocolVersion < 8`; Tatar ignores such peers.

Pseudo-steps for Rust: `spki_a = x509.tbs.subject_pki.raw` (via `x509-parser`) for both certs; `let (hi,lo) = if a < b {(b,a)} else {(a,b)}` (Vec<u8> `Ord` is lexicographic unsigned, same as Java `compareUnsigned` and Qt `QByteArray::operator<` for these inputs); `sha256(hi ++ lo ++ ts.to_string().as_bytes())[..4]` hex upper.

### 5.4 Security notes
- Only 32 bits of code: users must compare both screens; show the code on both sides; auto-reject after the 25-30 s timers; do not allow more than one pending pairing per device; rate-limit pairing attempts per IP (the reference rate-limits connections, not pair requests).
- Never accept an incoming pair request without explicit user confirmation (CLAUDE.md rule 6).

### 5.5 Worked test vector (from AND `PairingHandlerTest.kt`, re-verified)
Two EC P-256 certificates (the base64 certs are in the Android test file; subjects `CN=ee061a75_e403_4ecc_9261_5ffe2722f698` and `CN=157bbc28_6f2b_4b16_bd02_13345c02543c`). Their SPKI DER (hex, 91 bytes each):
```
A = 3059301306072a8648ce3d020106082a8648ce3d03010703420004aa3882939b98fac7c0ca8184a422d2e68c28085cfd2090e5ba6e656f9680c6b189efa982cc1467bb616f20f254ffa8f05311f0ce33852943c5fa8e2744f97ae7
B = 3059301306072a8648ce3d020106082a8648ce3d03010703420004395b9debac3269b3dbffb2ccd07f478bb6fa068916dfb18fac9cee882497e2d6915e1c3423da7004f9a4f25187a5adcc75788baac451e725316dfc7a97c2e02f
```
A > B (first differing byte `aa` vs `39`), so `data = A || B || ts`.
| timestamp | verification key |
|---|---|
| `1737228658` | `54DC916E` (same for A,B and B,A order) |
| `2737228658` | `8C07153A` |
| (v7, no timestamp) | `F3900DB5`; `A,A` -> `97A75917` |
Use these as a unit test (embed the two SPKI hex strings directly).

---

## 6. Plugin packets

General rules:
- Plugin packets may only be exchanged with **paired** devices (both apps drop/answer-with-unpair otherwise).
- A sender checks the peer's `incomingCapabilities`; **Android refuses to send any packet whose type is not in the peer's advertised `incomingCapabilities`** (`Device.sendPacketBlocking` -> `supportsPacketType`; KDE does the same, `Device::sendPacket`). So if Tatar does not advertise a type as incoming, the phone will never send it. Unknown incoming types: log and ignore (and close any payload).
- Android's number/boolean reads are lenient (strings like `"true"` and numbers as strings are accepted); KDE is similar through QVariant. Phone-originated values can arrive as unexpected JSON types: parse leniently (e.g. `isCancel` from Android's telephony arrives as the **string** `"true"`).

### 6.1 `kdeconnect.ping` - both directions
Body: optional `message` (string). With no body keys the phone shows the text "Ping!" (AND `plugins/ping/PingPlugin.kt`). Tatar sends `{}` (or `{"message": "..."}`). On receipt show a notification with `message` or "Ping!". Capabilities: incoming and outgoing `kdeconnect.ping` on both sides.

### 6.2 `kdeconnect.findmyphone.request` - Tatar -> phone only
Body: `{}` (empty). The phone starts ringing at full alarm volume until the user dismisses it; the same packet type toggles it (the desktop sends the same request again... **[UNVERIFIED]** whether a second request stops ringing; Android's code only shows an activity/notification with a "found it" action and stops locally). Tatar advertises it only in **outgoing**; Android advertises it in incoming (`FindMyPhonePlugin.supportedPacketTypes`) and has no outgoing. KDE `plugins/findmyphone/kdeconnect_findmyphone.json`: outgoing `kdeconnect.findmyphone.request`, supported-incoming none.

### 6.3 `kdeconnect.battery` - phone -> Tatar
Body (AND `plugins/battery/BatteryPlugin.kt`, KDE `plugins/battery/batteryplugin.cpp`):
| Field | Type | Meaning |
|---|---|---|
| `currentCharge` | integer | Percent 0-100 (Android computes level*100/scale; missing in malformed packets -> treat as unknown, KDE default -1) |
| `isCharging` | boolean | Plugged in |
| `thresholdEvent` | integer | `0` = none, `1` = battery just became low (Android sends `1` once when Android's "battery low" broadcast fires; reset to 0 on following packets). KDE ignores the low event if `isCharging`. |
- Android sends one packet when its battery plugin is loaded for the device (i.e. right after pairing/connection), and again whenever charge %, charging state or threshold changes. It does not need to be requested.
- `kdeconnect.battery.request`: **not present in kdeconnect-kde master or kdeconnect-android** (KDE's json lists only `kdeconnect.battery` in both directions; AND battery plugin has only `kdeconnect.battery`). **IOS does use it**: IOS incoming/outgoing capabilities include `kdeconnect.battery.request`, and its code sends it with body `{"request": true}` to ask a peer for an update (IOS `Plugins and Plugin Views/Battery/Battery.swift`). IOS answers with a normal `kdeconnect.battery` packet. For iOS support Tatar should advertise outgoing `kdeconnect.battery.request` and send `{"request": true}` after link-up. **[UNVERIFIED]** iOS behaviour end-to-end.
- Notifications required by CLAUDE.md: below 15 % (not charging) and "full" (100 % while charging): derive from `currentCharge`/`isCharging` ourselves; the phone's own `thresholdEvent` is only the Android low-battery flag (Android's threshold is ~15 % per system; do not rely on it).

### 6.4 `kdeconnect.clipboard` and `kdeconnect.clipboard.connect` - both directions
- `kdeconnect.clipboard`: body `{"content": "<text>"}`. Sent whenever the sender's clipboard changes (and the user allows auto-sync). Receiver sets its clipboard to `content` unconditionally.
- `kdeconnect.clipboard.connect`: body `{"timestamp": <integer ms since epoch>, "content": "<text>"}`. Sent **once right after the plugin is loaded for a device (i.e. link up / pairing)** to resolve which clipboard is newer. `timestamp` = the instant the sender's clipboard last changed, in **milliseconds**; `0` = unknown. Receiver rule (same in AND `ClipboardPlugin.kt` and KDE `clipboardplugin.cpp`): if `timestamp == 0` or `timestamp < ownClipboardUpdateTimestamp` -> ignore; otherwise, if `content` is present, set the local clipboard to it. Tatar must track `updateTimestamp` (ms) for its own clipboard, updating it when the local clipboard changes **and when it is set from the network** (to avoid echo loops, remember the last text we applied and do not re-send it).
- KDE additionally has `kdeconnect.clipboard.file` (files/images via payload); Android has no such type. Not in Tatar v1.
- Android's sensitive-content handling: its plugin has an option "skip sensitive" for passwords (Android API); no field in the packet marks sensitivity. Tatar's own filter (CLAUDE.md rule 4) is purely local: never send concealed/transient content.
- UI on Android: a "Send clipboard" button/menu appears when the plugin is loaded (Android 10+ cannot read the clipboard in the background so pushes from phone to desktop are mostly manual). Pushes from desktop to phone are applied directly.

### 6.5 `kdeconnect.share.request` (+ `.update`) - both directions
Receive side (what the phone sends us), Android sender `helpers/FilesHelper.kt` `uriToNetworkPacket`, `plugins/share/SharePlugin.kt`, `CompositeUploadFileJob.kt`:
| Variant | Fields |
|---|---|
| File | `filename` (string, no path - but **sanitise anyway**), `lastModified` (ms, integer, Android: file mtime; may be absent), `numberOfFiles` (integer, total in this batch), `totalPayloadSize` (integer, total bytes of the batch, excluding unknown sizes), plus top-level `payloadSize` and `payloadTransferInfo: {"port": N}` (section 7). Optional `open` (boolean) = "open after receiving". Android does not send `creationTime`. A zero-length file arrives with **no** `payloadSize`/`payloadTransferInfo` at all (Android's `hasPayload()` is false when size is 0): create an empty file. |
| Text | `text` (string) -> put in the clipboard and notify (KDE does exactly that); no payload |
| URL | `url` (string) -> open if http/https/mailto (KDE restricts to these; do the same, never `file:` or custom schemes) |
- Priority when several keys are present (AND `onPacketReceived`): `filename` > `text` > `url`; KDE: payload-or-filename > `text` > `url`.
- `kdeconnect.share.request.update`: body `{"numberOfFiles": N, "totalPayloadSize": B}`. Sent by the sender **before/while** a multi-file batch is sent, to refresh the batch totals (Android's `CompositeUploadFileJob.sendUpdatePacket`, KDE `CompositeUploadJob::sendUpdatePacket`). The receiver uses it for aggregate progress. **Android actually sends this packet but does not declare it in outgoing capabilities; because Android's sender-side filter checks the *peer's incoming* capabilities, Tatar must advertise `kdeconnect.share.request.update` in `incomingCapabilities` or the update packets are never sent.** (It is harmless if Tatar ignores their content.)
- File batching: for a batch of N files the sender emits N `share.request` header packets, each with its own payload connection (7). Android announces files one after another (each with a fresh listening port); KDE keeps one listening port per batch and may announce file k+1 while file k is still transferring (receiver connects immediately but the sender only serves connections in order). Tatar as receiver: connect to the port in each header, serialise per-file downloads in announcement order, tolerate headers arriving while a download is running.
Send side (Tatar -> phone), mirror of KDE `plugins/share/shareplugin.cpp`:
  - Text: `{"text": "..."}`; URL: `{"url": "https://..."}`.
  - File: `filename` (base name only), `creationTime` (ms; KDE sends it, Android ignores), `lastModified` (ms), `open` (boolean, false by default), `numberOfFiles`, `totalPayloadSize` (KDE sets these on every header; Android falls back to 1 file / payload size when `numberOfFiles` is absent), top-level `payloadSize` (bytes, or `-1`), `payloadTransferInfo: {"port": N}`. Send a `share.request.update` first/while queuing more files.
- Incoming filename handling (CLAUDE.md rule 3): take the part after the last `/`, additionally strip `\`, reject empty, `.`, `..`, leading `.` (hidden), NUL and control characters; reject if the resolved path escapes the download dir; if the name exists, append ` (1)`, ` (2)` before the extension (KDE auto-renames the same way). Apply `lastModified` (ms) as mtime if present and non-zero.
- Android UI: "Send file(s)" button and system share-sheet target appear when the share plugin is loaded (needs the capability overlap in section 8); the phone shows its own progress notification and a "cancel" action.

### 6.6 `kdeconnect.telephony` - phone -> Tatar (incoming calls)
Body (AND `plugins/telephony/TelephonyPlugin.kt`, KDE `plugins/telephony/telephonyplugin.cpp`):
| Field | Type | Notes |
|---|---|---|
| `event` | string | `"ringing"` (call incoming), `"talking"` (call answered / off-hook), `"missedCall"` (sent after an unanswered ring ended), `"sms"` (legacy SMS notice - ignore, out of scope) |
| `phoneNumber` | string | Optional (absent if hidden/unknown) |
| `contactName` | string | Optional; Android sets it to the phone number when it has no READ_CONTACTS permission but has a number; absent if there is neither |
| `phoneThumbnail` | string | Optional base64 of a JPEG contact photo (large - the 32 MiB packet cap applies) |
| `isCancel` | bool-ish | Android sends the **string** `"true"` (`"isCancel": "true"`); KDE reads it as bool. Parse as bool-or-string. Meaning: the previous `ringing` or `talking` event has ended (the call ended or was dismissed). |
- Sequence from Android: `ringing` -> (call ends) -> a repeat of the last packet with `isCancel` set -> and, if the call was never answered, an additional `missedCall` packet. For answered calls: `ringing`, `talking`, then `talking` with `isCancel`.
- For "mute when a call comes in": mute on `event == "ringing"` (and not `isCancel`); restore on `isCancel`, on `talking` (optional) or on connection loss. The phone only sends these if its user granted phone-state/call-log permission to KDE Connect.
- `kdeconnect.telephony.request_mute` (desktop -> phone, empty body, `AND TelephonyPlugin.PACKET_TYPE_TELEPHONY_REQUEST_MUTE`): asks the **phone** to silence its ringer. Not needed for v1 (muting is local); declare it in outgoing only if Tatar offers such a button.

### 6.7 `kdeconnect.notification` - phone -> Tatar (notification mirroring)
Verified against kdeconnect-android `plugins/notifications/NotificationsPlugin.kt` (supportedPacketTypes = `kdeconnect.notification.request`, `.reply`, `.action`; outgoingPacketTypes = `kdeconnect.notification`) and kdeconnect-kde `plugins/notifications/notification.cpp` (parse side).
Phone -> desktop `kdeconnect.notification` body:
| Field | Notes |
|---|---|
| `id` | String key, unique per notification on the phone; stable across updates. |
| `isCancel` | `true` (sent with only `id`): the phone removed the notification. Tatar: forget it, show nothing. |
| `appName`, `title`, `text`, `ticker` | Strings; absent when the user blocked contents for that app on the phone (then only id/appName/actions/time/silent are sent). For group chats Android sets `title` = group name and `text` = `"<sender>: <message>"`. |
| `silent` | Android: `isPreexisting \|\| (isUpdate && FLAG_ONLY_ALERT_ONCE)`. So `silent: true` = notification that already existed when the plugin synced (e.g. after `request: true`) or a re-post that must not alert again. KDE builds it but does not raise the event. Tatar: do not show. (`onlyOnce` is NOT a field in the Android packet; the notion is carried by `silent`.) |
| `isClearable`, `time` (ms as string), `actions` (array of titles), `requestReplyId`, `conversation`, `groupName` | Not used by Tatar. |
| `payloadHash` + payload | Icon PNG, sent only when the icon hash changed. Tatar ignores icons and never opens the payload socket (the phone's payload server simply times out). |
Android skips ongoing / foreground-service / group-summary / transport notifications itself and KDE Connect's own.
Desktop -> phone (NOT implemented): `kdeconnect.notification.request` with `{request:true}` (re-send all current, they come back with `silent:true`, so useless for mirroring) or `{cancel:"<id>"}` (dismiss on the phone); `kdeconnect.notification.action` (`key`, `action`); `kdeconnect.notification.reply` (`requestReplyId`, `message`).
Tatar: advertises only **incoming** `kdeconnect.notification` (Android loads the plugin because our incoming matches its outgoing, see 8); dedupes on (device, id, title+text), caps to 5 shown per 10 s, sanitises/length-limits all fields, never logs content. Settings `notificationMirror` (packets dropped when off; capability stays advertised) and `notificationHideContent`.
Note: Android also has a separate `receivenotifications` plugin (the *phone* receiving from a desktop); irrelevant here. The phone user must grant notification-listener access to KDE Connect.
### 6.9 `kdeconnect.runcommand`
Verified 2026-10-28 against KDE `plugins/runcommand/runcommandplugin.cpp` + `kdeconnect_runcommand.json` and AND `plugins/runcommand/RunCommandPlugin.kt` + `CommandEntry.kt` (`master`). The phone is the "remote control", the computer is the executor.
- **Desktop -> phone, `kdeconnect.runcommand`**: body `{"commandList": "<string>", "canAddCommand": true}`. `commandList` is a **JSON-encoded string** (not a nested object) of an object keyed by command id: `{"<id>": {"name": "...", "command": "..."}}`. Android decodes it with `Map<String, {name, command}>` and uses the map key as the command key; entries that fail to parse are skipped. Empty list = `"{}"`. `canAddCommand` (bool) makes Android show an "add command" button that sends `setup`. KDE sends the list right after the plugin connects and whenever its config changes.
- **Phone -> desktop, `kdeconnect.runcommand.request`**, exactly one of these body keys (KDE checks in this order): `requestCommandList: true` (Android sends it when the plugin loads; reply with the list), `key: "<id>"` (run that command), `setup: true` (open the command configuration UI on the desktop), `stop: true` (terminate running commands; Tatar ignores it).
- **Security**: the phone only names a key; the command text always comes from the desktop's own list. Tatar looks the key up in its local settings only; an unknown key is ignored and logged (without command text). KDE's `/bin/sh -c` (Windows: `cmd /C`) is mirrored. Android's UI shows the `command` text as subtitle, so the list sent to the paired phone includes it.
- **Output packets**: KDE also sends `kdeconnect.runcommand.output` (`commandStarted`, `commandOutput` with stdout/stderr lines, `commandFinished`); Android only displays them. Tatar does **not** send them (no command output leaves the machine) and does not advertise them.
- **Capabilities**: Android plugin: incoming `kdeconnect.runcommand`, `kdeconnect.runcommand.output`; outgoing `kdeconnect.runcommand.request`. KDE json: outgoing `kdeconnect.runcommand`, `kdeconnect.runcommand.output`; supported incoming `kdeconnect.runcommand.request`. Tatar: incoming `kdeconnect.runcommand.request`, outgoing `kdeconnect.runcommand` (enough to load the phone's plugin).
- **[UNVERIFIED]** end-to-end behaviour with the real phone (Tatar side is unit-tested only); Windows command execution (`cmd /C`) not run on this machine.

---

## 7. Payload transfer (file data)

A packet that carries data has top-level `payloadSize` and `payloadTransferInfo`; the bytes do **not** travel on the control socket but on a separate TCP connection.

### 7.1 Sender side
1. Open a TCP listener on the first free port in **1739..1764** (Android starts at 1739; KDE's composite upload job starts at `MIN_PORT`=1716 of the shared range but any free port in 1716..1764 is accepted by the receiver - receivers only read the port number). Recommended: start at 1739 like Android.
2. Send the header packet on the control socket with `payloadSize` (bytes; `-1` if unknown) and `payloadTransferInfo = {"port": <listener port>}`.
3. Wait for the receiver to connect: **Android 10 s timeout; KDE 30 s**. After the timeout close the listener (transfer failed, "receiver never connected").
4. Accept **one** connection (KDE uses a single listener for a whole batch and accepts connections in file order; Android uses one listener per file).
5. Upgrade the accepted socket to TLS as **TLS server** (`startServerEncryption` in KDE `compositeuploadjob.cpp`; `clientMode = false` in AND `LanLink.sendPayload`), using exactly the same certificate-pinning rules as the control connection for that deviceId (paired -> peer cert must equal the stored one; client cert is required).
6. Stream the bytes, then close the socket (closing = end of data). Android stops the stream early if cancelled.

### 7.2 Receiver side
1. On a packet with non-empty `payloadTransferInfo`, read `port` (integer).
2. Open a TCP connection to **the same IP address as the control connection's peer** (not a configurable address) on `port`.
3. Upgrade to TLS as **TLS client** (`connectToHostEncrypted` in KDE `landevicelink.cpp`; `clientMode = true` in AND `LanLink.receivedNetworkPacket`), present our certificate, verify the server certificate against the pinned one of this paired device.
   - So on payload sockets the TLS roles follow the TCP roles in the *normal* direction (TCP connector = TLS client, TCP listener = TLS server) - **the opposite of the control connection.**
4. Read until: exactly `payloadSize` bytes received, or (if `payloadSize == -1`) until the sender closes the stream. Behaviour on mismatch (KDE `core/filetransferjob.cpp`, AND `CompositeReceiveFileJob`): for a known size, stopping short is a failure (delete the partial file, "incomplete file"); receiving more than announced is tolerated by KDE (extra bytes kept, logged), Android treats `received < payloadSize` as failure. Tatar: fail if short, keep reading until EOF otherwise, cap at the announced size when known (recommended) or the user's disk-space policy when `-1`.

### 7.3 Unknown size (`payloadSize = -1`)
Android produces this when the content provider doesn't report a size (`sizeDefault = -1`). `totalPayloadSize` excludes such files. Treat progress as indeterminate; end of data = peer closes the TLS stream cleanly (read EOF after close_notify; also accept abrupt EOF but mark it as "unverified size").

### 7.4 Cancellation
There is no protocol packet for cancelling. Cancel = close the payload socket (receiver) or close the listener/socket (sender). The other side sees the stream end early or an I/O error; a receiver with a known size treats it as incomplete. Android's sender reacts to its own cancel by flagging the packet and stopping the loop; KDE's `doKill` closes the listener and the input. Tatar must delete partial files on cancel/failure.

### 7.5 Zero-length payloads
A zero-length file has **no** `payloadSize`/`payloadTransferInfo`: create the empty file; do not connect.

---

## 8. Capabilities Tatar should advertise (v1)

Rules (AND `Device.kt`, `plugins/PluginFactory.kt`):
- A phone plugin is loaded for a paired, reachable device if **any** of: our *outgoing* contains one of that plugin's *incoming* types, or our *incoming* contains one of that plugin's *outgoing* types (`pluginsForCapabilities`). A plugin that is not loaded shows no UI and sends nothing.
- The phone only sends types listed in our `incomingCapabilities` (its send-side filter). Our `outgoingCapabilities` are used by the phone only for the plugin-matching rule above.

Tatar v1:
```
incomingCapabilities (what we can receive):
  kdeconnect.ping
  kdeconnect.battery
  kdeconnect.clipboard
  kdeconnect.clipboard.connect
  kdeconnect.share.request
  kdeconnect.share.request.update
  kdeconnect.telephony
  kdeconnect.notification           (6.7; mirroring)
  kdeconnect.runcommand.request

outgoingCapabilities (what we send):
  kdeconnect.ping
  kdeconnect.findmyphone.request
  kdeconnect.clipboard
  kdeconnect.clipboard.connect
  kdeconnect.share.request
  kdeconnect.share.request.update
  kdeconnect.battery.request        (optional; only useful for iOS peers, see 6.3)
  kdeconnect.runcommand
```
Why each (what Android shows because of it):
| Capability | Phone effect |
|---|---|
| in `kdeconnect.ping` + out `kdeconnect.ping` | Ping plugin loads; "Send ping" menu entry on the phone; phone shows our pings as notifications |
| in `kdeconnect.battery` | Battery plugin loads and the phone starts sending battery packets to us (and shows nothing for us, since we never send battery) |
| out `kdeconnect.findmyphone.request` | FindMyPhone plugin loads on the phone (it is the ringing side) |
| in/out `kdeconnect.clipboard`, `kdeconnect.clipboard.connect` | Clipboard plugin loads: phone sends initial `clipboard.connect` and updates; "Send clipboard" button on the phone |
| in `kdeconnect.share.request` (+`.update`), out `kdeconnect.share.request` | Share plugin loads: "Send files" button and share-sheet target on the phone; phone can send us files, text, URLs |
| in `kdeconnect.notification` | Notifications plugin loads on the phone (needs notification-listener access) and mirrors notifications to us |
| out `kdeconnect.runcommand`, in `kdeconnect.runcommand.request` | RunCommand plugin loads on the phone: shows our command list as buttons; tapping sends `key` (see 6.9) |
| in `kdeconnect.telephony` | Telephony plugin loads on the phone (needs the user's phone/call-log permissions), which then sends call events to us |
Do **not** advertise anything we cannot handle (CLAUDE.md rule): e.g. not `kdeconnect.clipboard.file`, `kdeconnect.mpris*`, `kdeconnect.sms.*`, `kdeconnect.runcommand.output`, `kdeconnect.notification.request/.action/.reply`, `kdeconnect.mousepad.*`, `kdeconnect.sftp*`.
iOS peers' capabilities (for tolerance): ping, share.request(+update), findmyphone.request, battery(+request), clipboard(+connect), runcommand; no telephony. So with an iPhone, no call-mute feature.
Packet types the phone declares as incoming that Tatar can use: Android incoming: `kdeconnect.ping`, `kdeconnect.findmyphone.request`, `kdeconnect.battery`, `kdeconnect.clipboard`, `kdeconnect.clipboard.connect`, `kdeconnect.share.request`, `kdeconnect.share.request.update`, `kdeconnect.telephony.request_mute`. Before sending any packet, check the peer's `incomingCapabilities` contains the type (KDE does, Android does).

---

## 9. Keepalive, reconnect, and other acceptance requirements

- **No application-level heartbeat/ping packet exists** in the LAN link. Liveness is detected by TCP: KDE enables TCP keepalive (idle 30 s, interval 10 s, 3 probes, plus `TCP_USER_TIMEOUT` 60 s where available) on all sockets (`configureSocket`); Android calls `setKeepAlive(true)` and uses a 10 s read timeout that it *ignores* (loops again). Tatar should enable TCP keepalive with the KDE values (30/10/3, via `socket2`) so dead links are noticed within ~60 s.
- On link loss the device becomes "unreachable but still paired". Reconnect happens only through discovery events: the phone re-broadcasts UDP on network change (and re-announces mDNS); Tatar also should broadcast UDP on start, on network-change events (interface up/down, Wi-Fi join, wake from sleep) and, for paired devices that are unreachable, retry on a timer (suggested: every 30 s with jitter, bounded by the 500 ms rate limit) by sending a unicast UDP identity to the last known IP and by connecting directly to the last known IP:port. Rate-limit retries.
- A device can have several TCP links in KDE; with one deviceId only one "active" link is used per implementation here (Android's LanLink is a single socket).
- **Reject protocol < 8:** both references drop UDP/mDNS/TCP identities with `protocolVersion < 8` (KDE `deviceDiscovered` and `tcpPacketReceived`). Note Android's own code still contains a pre-8 legacy path (it skips the secure identity re-exchange if the peer announced < 8), so a peer can theoretically be < 8 - Tatar must not implement the legacy path and must close such connections. Tatar must also send exactly `protocolVersion: 8` everywhere (a value < 8 puts Android into legacy mode).
- **Downgrade check** for paired devices (3.4).
- **Trusted-device cert required:** if a device is marked trusted but no certificate is stored, refuse (Android logs "should not happen").
- **Unpaired-device limits:** at most 42 concurrent unpaired links.
- **Timeouts to implement:** 1 s (KDE) / 10 s (AND) for peer's identity line after TLS - use 5 s; plain pre-TLS identity: 1 s on KDE, effectively 10 s on Android; Tatar: accept up to 5 s for an incoming line, and send ours promptly (<1 s) because KDE enforces 1 s on what it receives from us.
- **Packet types for pairing are "protocol packets"**: `kdeconnect.identity` and `kdeconnect.pair` are always allowed regardless of capabilities (both apps whitelist them).
- **Device type** value must be from the list in 1.2; use `laptop`/`desktop`.
- The `id` field must be a number (ms) - Android's deserialiser defaults it to 0 if missing, but send it.
- Android needs the **Local-network permission** on newest OS versions (it logs "missing ACCESS_LOCAL_NETWORK permission" and then neither broadcasts nor mDNS-discovers): if no phone shows up, the phone's user must grant it. Add it to Tatar's troubleshooting copy. On macOS, `NSLocalNetworkUsageDescription` and `NSBonjourServices` are required (CLAUDE.md section 5).
- macOS firewall/binding: bind UDP 1716 with `SO_REUSEADDR` (+ `SO_REUSEPORT` on macOS) and `SO_BROADCAST`; send broadcast from each interface's own IPv4 address (bind-per-interface) because a single wildcard socket may only leave on the default route interface.

---

## 10. Gotchas (read before implementing)

1. **TLS roles are inverted for the control connection, not for payload sockets.** Control: TCP initiator = TLS *server*. Payload: TCP initiator (receiver) = TLS *client*. Mixing these up gives silent handshake failures.
2. **Only the TCP initiator sends the plain identity**, and it must contain `targetDeviceId` + `targetProtocolVersion` (KDE refuses it otherwise). After TLS, **both** send the full identity immediately.
3. **Android TLS is capped at 1.2.** rustls config must enable TLS 1.2; custom verifier objects for both server (client-cert) and client (server-cert) roles; implement `verify_tls12_signature`/`verify_tls13_signature` with the real algorithms (use rustls's `WebPkiSupportedAlgorithms`), only skipping *chain/CA/name* validation. Test that rustls/webpki parses Android's cert (v3, serial 1, no extensions, ecdsa-with-SHA512, `CN/OU/O` subject); if webpki refuses it, verify the handshake signature with your own SPKI-based code (`x509-parser` + `ring`/`p256`).
4. **Pin by DER equality** for paired devices on every socket, including payload sockets; a mismatched cert is never "downgraded" to unpaired behaviour.
5. **Pairing verification key = SHA-256(larger_SPKI || smaller_SPKI || ascii(timestamp_seconds)), first 4 bytes, uppercase hex.** The timestamp is the one in the request (seconds). SPKI, not the whole cert.
6. **Do not send anything but `identity`/`pair` to an unpaired device, and do not send a redundant `pair:true` to a paired one** - the phone will unpair you.
7. **Android drops what you didn't advertise:** capabilities are the only way to receive battery, telephony, clipboard, share updates. Missing `kdeconnect.share.request.update` in `incomingCapabilities` means no batch totals.
8. **Mixed JSON types from the phone:** `isCancel` is the string `"true"`; `targetProtocolVersion` is a string; numbers can come as ints/longs. Parse leniently; accept both numbers and numeric strings.
9. **Android's mDNS path is "send me a UDP packet", not a TCP connect.** Always keep UDP 1716 open and answer by connecting out.
10. **Android's rate limit is 1 s per IP/deviceId** - connecting twice in a row from a retry loop gets the second attempt silently dropped.
11. **Never regenerate the certificate/id casually**: the deviceId is the CN and the phone pins your cert; a new cert makes the phone's saved trust useless (and requires re-pairing).
12. **Names:** filter `" ' , ; : . ! ? ( ) [ ] < >` from `deviceName` before advertising or the peer may display a different name or reject an empty one; max 32 chars.
13. **Payload port discovery:** connect to the control socket's peer IP, not an address in the packet. The sender may take up to 10 s (Android) before giving up waiting for you; do not delay connecting.
14. **Zero-byte files** come without any payload fields.
15. **deviceId characters:** make ours exactly 32 lowercase hex chars; peers may have `-`/`_` and up to 38 chars; KDE rewrites non-`[A-Za-z0-9_]` in ids it receives - compare CN and id with the same normalisation if you want to interoperate with KDE-style hosts (the phone does not normalise).
16. **iOS differs slightly**: no telephony, uses `kdeconnect.battery.request`; same TLS roles and v8 flow.
17. **Secrets in logs:** CLAUDE.md rule 7 - never log clipboard text, file names are okay only if non-sensitive (prefer not), never log keys.

---

## 11. Open questions / unverified

1. Whether Android accepts a peer cert signed `ecdsa-with-SHA256` (vs the reference SHA-512). Expected yes (no code inspects it; JVM allows SHA-256), but not tested with a real phone.
2. Whether `rustls`/`webpki` accepts Android's/KDE's certificates (v3 without extensions, serial `01`, SHA-512 ECDSA) for the handshake-signature check; may require a custom signature verifier (see Gotcha 3).
3. TLS behaviour of the **iOS** app beyond roles (supported TLS versions, whether it requires client auth in all cases) - only read its `LanLinkProvider.m` TLS settings.
4. iOS `kdeconnect.battery.request` end-to-end behaviour (what it replies, whether it also pushes unprompted) - read the iOS plugin only, not tested.
5. Whether a second `kdeconnect.findmyphone.request` stops the phone ringing. In current Android code the ringing is stopped from the phone's own UI; the packet appears to be a one-shot trigger.
6. Exact UDP source-address rules on multi-homed hosts (macOS: whether one `SO_BROADCAST` socket bound to `0.0.0.0` reaches all interfaces; KDE works around it by per-interface sockets).
7. Exact Android behaviour when it receives an in-TLS identity with extra/unknown fields or additional capabilities beyond the list it knows - believed ignored; not tested.
8. The symmetric connection-race outcome (3.6) is inferred from code, not observed.
9. Android's character counting for the 32-char `deviceName` limit (UTF-16 units via Kotlin `take`) vs Qt (`left`, UTF-16 units) - both UTF-16 code units; names with non-BMP characters can be cut mid-surrogate; avoid long non-ASCII names.
10. Whether KDE (desktop) peers other than master (older v1.4/v24.x releases) accept missing `deviceType` in the plain TCP identity - irrelevant for phones, relevant only if we also want to talk to desktops.
11. mDNS TXT `type` key: only Android writes it, nobody reads it; we include it harmlessly.
12. The exact rate at which Android re-broadcasts when Wi-Fi flaps and whether it ever periodically rebroadcasts (no periodic timer found in the code read).

## 12. Reference file index (for later re-verification)

KDE (`kdeconnect-kde`): `core/backends/lan/lanlinkprovider.cpp|.h`, `core/backends/lan/landevicelink.cpp`, `core/backends/lan/compositeuploadjob.cpp`, `core/backends/lan/uploadjob.cpp`, `core/backends/lan/mdnshdiscovery.cpp`, `core/backends/pairinghandler.cpp|.h`, `core/deviceinfo.h`, `core/networkpacket.cpp`, `core/kdeconnectconfig.cpp`, `core/sslhelper.cpp`, `core/device.cpp`, `core/filetransferjob.cpp`, `plugins/{ping,findmyphone,battery,clipboard,share,telephony}/*`.
AND (`kdeconnect-android`, `src/main/java/org/kde/kdeconnect/`): `backends/lan/{LanLinkProvider,LanLink,MdnsDiscovery}.kt`, `PairingHandler.kt`, `DeviceInfo.kt`, `Device.kt`, `NetworkPacket.kt`, `helpers/security/{SslHelper,RsaHelper}.kt`, `helpers/{DeviceHelper,NetworkHelper,BoundedLineReader,FilesHelper}.kt`, `plugins/{PluginFactory,ping/PingPlugin,findmyphone/FindMyPhonePlugin,battery/BatteryPlugin,clipboard/ClipboardPlugin,share/SharePlugin,share/CompositeUploadFileJob,share/CompositeReceiveFileJob,telephony/TelephonyPlugin}.kt`, test `src/test/java/org/kde/kdeconnect/PairingHandlerTest.kt`.
IOS: `KDE Connect/ObjC Network Backend/lanBackend/LanLinkProvider.m`, `KDE Connect/Swift Backend/KdeConnectSettings.swift`, `.../Plugins and Plugin Views/Battery/Battery.swift`.
