**English** | [简体中文](API.zh-CN.md)

# FarSail coordinator API v1

The coordinator implements JSON over HTTP. The local server binds `127.0.0.1:8787`; remote deployments must put a TLS validating reverse proxy in front of the loopback listener. Clients must use a configurable HTTPS base URL and verify the server certificate. This API does not carry screen, input or file bytes.

All identifiers are UUID strings. Auth tokens and invitation codes are opaque 256-bit random values, returned once and stored as SHA-256 digests. Set `Authorization: Bearer <access_token>` for user endpoints. Set `Authorization: Bearer <device_token>` for device-only endpoints. A remote request uses user Bearer plus `X-Farsail-Device-Token: <source_device_token>`. A grant inspection uses grant Bearer plus a participating device token in that header. Never put credentials in a URL, log or WebView localStorage.

Errors use an HTTP status and `{ "error": "..." }`: 400 malformed/expired input, 401 missing or invalid credential, 403 authenticated but disallowed, 404 out-of-scope resource, 409 conflicting or stale state, 429 rate limit. Most successful mutations return `200` with JSON or an empty body. `GET /healthz` checks PostgreSQL and returns `200` when ready.

## Account

| Method and path                      | Body / result                                                                        | Credential                                |
| ------------------------------------ | ------------------------------------------------------------------------------------ | ----------------------------------------- |
| `POST /v1/auth/register`             | `{email,password,invite_code?}` → `{id}`; password 12–1024 bytes                     | none; code required in `invite_only` mode |
| `POST /v1/auth/verify`               | `{token}`; consumes email token                                                      | none                                      |
| `POST /v1/auth/verify/resend`        | `{email,password}`; reissues verification mail for unverified account                | none                                      |
| `POST /v1/auth/login`                | `{email,password}` → `{access_token,refresh_token,session_id,access_expires_in}`     | none                                      |
| `POST /v1/auth/refresh`              | `{refresh_token}` → new token pair; reuse revokes session                            | refresh token                             |
| `POST /v1/auth/logout`               | revokes current login and its remote sessions                                        | user                                      |
| `POST /v1/auth/password`             | `{old_password,new_password}`; revokes all account sessions and remote grants        | user                                      |
| `POST /v1/auth/recovery/request`     | `{email}`; generic success and one-time mail if account exists                       | none                                      |
| `POST /v1/auth/recovery/complete`    | `{token,password}`; consumes all pending recovery tokens and revokes sessions/grants | recovery token                            |
| `GET /v1/me`                         | `{id,email,role,session_id}`                                                         | user                                      |
| `GET /v1/auth/sessions`              | current account's login sessions                                                     | user                                      |
| `POST /v1/auth/sessions/{id}/revoke` | revokes owned login and its grants                                                   | user                                      |

Email is normalized to lowercase. Verification lasts 24 hours, recovery 30 minutes, access 15 minutes, refresh 30 days. Argon2id password work is bounded to four blocking workers. Registration, login, recovery, challenge and invitation flows have PostgreSQL backed limits; a TLS proxy should also enforce per-IP limits. If mail delivery fails after registration, `verify/resend` allows recovery with the account password.

## Device

The client generates a long-term Ed25519 keypair and keeps the private key in platform secure storage. Send the 32-byte public key as lowercase hex. `POST /v1/devices/challenge` with `{public_key}` returns `{challenge_id,nonce,message,expires_in}`. Sign the UTF-8 `message` exactly as returned and call `POST /v1/devices/bind` with `{challenge_id,signature,name,platform,can_host,can_files}`. Signature is 64-byte lowercase hex; `platform` is `windows`, `android`, or `ios`. The response is `{id,device_token}`. The challenge is single-use and expires after five minutes. Same-owner rebind rotates the device token; another owner cannot take over the same public key. Admin-disabled devices cannot rebind until an admin explicitly enables them.

| Method and path                                         | Body / result                                                                                                                                        | Credential |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| `GET /v1/devices?limit=50&after=<uuid>&host_only=false` | all currently bound own devices, ordered by UUID, including offline and admin-disabled; `online`, `enabled`, `last_seen_at` included                 | user       |
| `GET /v1/devices/{id}`                                  | own bound device                                                                                                                                     | user       |
| `PATCH /v1/devices/{id}`                                | `{name}`                                                                                                                                             | user       |
| `DELETE /v1/devices/{id}`                               | unbinds an enabled own device, revokes credential, invitations and grants                                                                            | user       |
| `POST /v1/devices/heartbeat`                            | `{generation:null}` starts a new connection generation; subsequent `{generation:n}` renews a 60-second lease                                         | device     |
| `POST /v1/devices/capability`                           | `{generation:n,can_host:true\|false}` updates a live device's host ability; false transactionally revokes pending/approved view and control requests | device     |

A stale generation receives 409 and cannot change the newer connection's lease. The device credential is tied to the login session that issued it; logout, login revocation, password change/recovery, and account disable invalidate it and clear presence. A later login signs a fresh bind challenge to obtain a new token. Unbind leaves an inactive historical row for audit references and frees its public key for a new explicit proof; admin disable retains the binding and requires admin enable before reproof. `device_token` cannot call account or list endpoints.

Windows desktop binding starts with `can_host=false` and `can_files=false`. Its native client probes DXGI and requires the local sharing switch before setting `can_host=true`. The coordinator requires a live matching heartbeat generation. On startup without sharing, the desktop clears a stale host declaration left by an abnormal exit. `can_files` remains false until WI-005.

## Invitation and remote authorization

Permissions are `view`, `control`, `files` and are distinct. An invitation is valid ten minutes, one use, and scoped to target device and permission. The requester must prove its own online source device. The target device must be online and capable. Same-owner requests need no invitation; cross-owner requests must consume a valid invitation. Every request still starts `pending`, including same-owner requests. Only the target's device credential can approve or deny. Admin role cannot approve.

| Method and path                    | Body / result                                                                                                     | Credential                                          |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| `POST /v1/invitations`             | `{target_device_id,permission}` → `{id,code,expires_in}`                                                          | target owner user                                   |
| `POST /v1/invitations/{id}/revoke` | revokes code and dependent grants                                                                                 | target owner user                                   |
| `POST /v1/remote/request`          | `{source_device_id,target_device_id,permission,invitation_code?}` → `{id,state:"pending"}`                        | requester user + source device header               |
| `GET /v1/remote/pending`           | target device's pending requests, including requester email and source device name for an informed local decision | target device                                       |
| `POST /v1/remote/{id}/decide`      | `{approve:true\|false}` → grant on approval, null on denial                                                       | target device                                       |
| `POST /v1/remote/{id}/renew`       | new grant token while prior 30-second grant remains live and checks still pass                                    | target device                                       |
| `POST /v1/remote/{id}/revoke`      | revokes request/grant                                                                                             | requester user or target device                     |
| `GET /v1/remote/{id}/grant`        | validates a live grant and returns session, permission, both public keys, nonce and expiry                        | grant token + participating device header           |
| `GET /v1/remote/{id}`              | pending/approved/denied/revoked/expired metadata; no grant token                                                  | requester/target owner user or participating device |
| `GET /v1/remote/sessions`          | latest 100 own or owned-target requests; no grant tokens                                                          | user                                                |

The 30-second grant is bound in PostgreSQL to the session ID, requester account login, both device identities, exact permission, random nonce and expiry. Inspection and renewal recheck account status, device state, presence lease, invitation revocation and login revocation. An expired grant cannot be renewed; start a new request and obtain fresh target approval. The target delivers its grant only inside a device-authenticated iroh connection; `GET /v1/remote/{id}` remains a status poll. Frame permissions are enforced in the native transport; screen/input/file handlers are still pending.

## WI-003 address signaling and transport validation

These routes require the device Bearer credential and `X-Farsail-Generation: <current heartbeat generation>`. A stale process cannot publish or discover. An address contains an iroh `EndpointAddr` serialized as JSON, with the endpoint ID equal to the registered Ed25519 public key. At most 16 IP/relay addresses and 2048 serialized bytes are accepted. Custom transports are not accepted. Relay URLs require HTTPS without URL credentials, query or fragment, and must appear in the coordinator's `FARSAIL_RELAY_URLS` comma separated allowlist. The allowlist is empty by default.

| Method and path                        | Body / result                                                                                                                     | Credential                                              |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| `POST /v1/devices/endpoint-address`    | `{generation,endpoint_addr}`; upserts only the current generation and never lets a delayed older generation overwrite a newer one | device + generation                                     |
| `GET /v1/remote/{id}/peer-address`     | `{endpoint_addr,generation,expires_in}` for the other device in a live approved session                                           | participating device + generation                       |
| `GET /v1/remote/{id}/transport-grant`  | same claims as grant inspection plus server-measured `expires_in`; binds to current device generation                             | grant Bearer + participating device header + generation |
| `POST /v1/remote/{id}/transport-renew` | rotates the target's live grant token, returned only to the target                                                                | target device + generation                              |

`peer-address` rechecks participating devices, logins, online leases, invitation and approval. It is scoped to the current session. Grant inspection's `expires_in` is measured against the coordinator clock; native transport subtracts elapsed local HTTP time and uses `Instant` for its cutoff. The older `/grant` and `/renew` routes remain for the WI-001 authorization contract, while transport uses generation-bound variants. See [TRANSPORT.md](TRANSPORT.md) for the authenticated exchange, frame limits, renewal and path reporting.

## Admin

An explicit local `cargo run -p farsail-coordinator -- bootstrap-admin <verified-email>` assigns the admin role to an existing verified, enabled account. There is no first-registrant promotion. Admin routes require a user access token and never issue a remote grant.

| Method and path                             | Function                                                                                                                   |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `GET /v1/admin/users?limit=50&after=<uuid>` | paged users                                                                                                                |
| `POST /v1/admin/users/{id}/enabled`         | `{enabled}`; disabling revokes sessions, device credentials, invitations and grants                                        |
| `POST /v1/admin/devices/{id}/revoke`        | disable device, rotate credential, revoke dependent grants                                                                 |
| `POST /v1/admin/devices/{id}/enabled`       | `{enabled:true}`; reenable requires owner to sign a fresh bind challenge                                                   |
| `GET/PUT /v1/admin/registration`            | `{value:"open"\|"invite_only"\|"closed"}`                                                                                  |
| `POST /v1/admin/signup-invitations`         | `{email}` → `{id,code,expires_in}`; code is returned once for manual delivery, lasts seven days and is scoped to the email |
| `DELETE /v1/admin/signup-invitations/{id}`  | revoke an unused signup invitation                                                                                         |
| `GET /v1/admin/sessions`                    | latest 100 login session metadata                                                                                          |
| `GET /v1/admin/audit`                       | latest 100 metadata events; no passwords, tokens, frames or input                                                          |

## Configuration

`FARSAIL_DATABASE_URL` is required. `FARSAIL_BIND` defaults to `127.0.0.1:8787` and rejects non-loopback addresses. `FARSAIL_MAIL_MODE=smtp-local` defaults to Mailpit on `127.0.0.1:1025` (set `FARSAIL_SMTP_PORT=51025` for local Compose); it refuses non-loopback SMTP hosts. `FARSAIL_MAIL_MODE=smtp-tls` requires `FARSAIL_SMTP_HOST`, `FARSAIL_SMTP_USER`, `FARSAIL_SMTP_PASSWORD`, `FARSAIL_MAIL_FROM`, and uses a TLS validating SMTP relay. `memory` requires explicit `FARSAIL_DEV_MEMORY_MAIL=1` and is for tests only. Audit events are metadata; a post-commit audit write error is logged but cannot turn a successful mutation into an ambiguous HTTP failure. Production deployment should provide durable audit monitoring, external per-IP rate limiting, backup and TLS proxy configuration before exposing the service.
<a id="内部-relay-准入wi-008a"></a>

# Internal relay admission (WI-008A)

`POST /internal/relay-access` is mounted only when `FARSAIL_RELAY_ACCESS_TOKEN` is set to at least 32 bytes. It requires a matching Bearer token and a 64-character hex `X-Iroh-NodeId`. This public key is proven by the iroh-relay connection handshake; callers should be trusted local relays only. Clients cannot call this endpoint themselves as a substitute for device proof. Only `200 true` indicates that the registered device is still bound/enabled, the user is verified/enabled, the binding login has not been revoked, and its refresh token has not expired. Unknown/disabled devices return `200 false`; a missing/incorrect Bearer token returns 401, an invalid public key returns 400, and a database failure returns 500 or times out after 2 seconds. The reverse proxy blocks the entire `/internal` path from public access.

This controls connection admission and is not a remote-control grant: an already connected relay does not continuously recheck admission. Application authorization, the 30-second lease, and revocation rules remain unchanged. All public `/v1/admin` endpoints continue to use the existing administrator authentication; a reverse proxy does not open anonymous management.
