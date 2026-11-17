# TONE3000 API v1 — Recon Reference

**Date:** 2026-11-14 (probes run 21:23–22:15 UTC)
**Scope:** TONE3000 v1 REST API, authentication, pagination, model downloads, rate limits, terms.
**Sources:** official docs (`https://www.tone3000.com/api`, `/api/terms`), live HTTP probes, the official example repo `tone-3000/t3k-api`, the independent Rust client `OpenSauce/tone3000-rs` (docs.rs, MIT), and one public model file download.

**Verification legend**

| Mark | Meaning |
|---|---|
| ✅ | Live-verified in this recon (no credentials needed) |
| 📄 | From official docs / reference clients; **not** live-verified here |
| ⚠️ | Discrepancy, undocumented behaviour, or open risk |

> **Credential status:** no TONE3000 publishable key, secret key, or user account was available in this environment. Every authenticated v1 success path is therefore documented, not live-verified. What *is* live-verified: routing, auth gates, OAuth error paths, the public trending feed, CORS, and the public model-file delivery route. Section 12 explains how to close the gap.

**Acceptance-criteria status**

| Criterion | Status |
|---|---|
| 1. Table of endpoints with purpose and auth requirements | ✅ §3 |
| 2. At least one verified example request/response per endpoint | ⚠️ Partially. Live ✅ for the public endpoint (`/tones/trending`), the auth gate on every protected endpoint (401 + exact body), OAuth error paths, CORS, and the file route. Success-path responses for authenticated endpoints are 📄 documented only (no credentials). See §12. |
| 3. Full metadata field list | ✅ §7 (Tone/Model/User/Make/Tag/Favorite/PaginatedResponse/Session + enums + NAM file metadata), with live-vs-documented marks |
| 4. Rate limits, caching/redistribution rules, unknowns/risks | ✅ §8, §9, §11 |

---

## 1. Base URLs and infrastructure

| Purpose | URL |
|---|---|
| v1 API base | `https://www.tone3000.com/api/v1` |
| OAuth authorize (redirect) | `https://www.tone3000.com/api/v1/oauth/authorize` |
| OAuth token (exchange / refresh / device poll) | `https://www.tone3000.com/api/v1/oauth/token` |
| OAuth device authorization | `https://www.tone3000.com/api/v1/oauth/device_authorization` |
| Device approval page | `https://www.tone3000.com/activate` |
| Model/IR file route (web app, **not** `/api/v1`) | `https://www.tone3000.com/api/files/models/{id}/{filename}` |
| Signed file host | `https://files.tone3000.com/{filename}?verify=<ts>-<sig>` |
| Image / avatar assets | `https://assets.tone3000.com/...` |
| Embed player | `https://www.tone3000.com/embed/player?tones=<id,...>` |
| API docs / terms | `https://www.tone3000.com/api`, `https://www.tone3000.com/api/terms` |
| API key settings | `https://www.tone3000.com/settings` |

Infrastructure observed ✅: `server: Vercel`, `x-matched-path`, `x-vercel-cache`, `x-vercel-id`; files served through Cloudflare (`server: cloudflare`, `cf-ray`).

CORS ✅:

- `access-control-allow-origin: *`
- GET endpoints: `access-control-allow-headers: *`, `access-control-allow-methods: *`
- POST endpoints: `access-control-allow-headers: Content-Type, Authorization`, `access-control-allow-methods: POST, OPTIONS`
- Preflight (`OPTIONS` with `Origin` + `Access-Control-Request-Method: GET`) → `204`, no body.

---

## 2. Authentication

### 2.1 Key types (Settings → API Keys)

| Key | Format | Role | Ship in client? |
|---|---|---|---|
| Publishable key | `t3k_pub_...` | OAuth 2.0 `client_id` for authorize/token/device requests | Yes (safe) |
| Secret key | `t3k_cs_...` | Server-only Bearer credential for direct API calls | **Never** |
| User access token | opaque | `Authorization: Bearer <token>` on every v1 call | Per-user, stored securely |
| Refresh token | opaque | Rotates; exchange at `/oauth/token` with `grant_type=refresh_token` | Store securely |

Environment variable names used by the reference implementations (names only — no values anywhere in this doc):

| Impl | Variables |
|---|---|
| `OpenSauce/tone3000-rs` examples | `T3K_PUB_KEY`, `T3K_ACCESS_TOKEN` (refresh token also supported) |
| `tone-3000/t3k-api` web example | `VITE_PUBLISHABLE_KEY`, `VITE_PUBLISHABLE_KEY_SELECT`, `VITE_PUBLISHABLE_KEY_LOAD`, `VITE_PUBLISHABLE_KEY_FULL`, `VITE_REDIRECT_URI`, `VITE_T3K_API_DOMAIN` |

### 2.2 OAuth flows

All user-facing flows use OAuth 2.0 + PKCE (`S256`) and issue the same access token.

| Flow | Use | Notes |
|---|---|---|
| Standard (no `prompt`) | connect account, full API | redirect flow |
| `prompt=select_tone` | user browses TONE3000 and picks a tone | callback includes `tone_id` |
| `prompt=load_tone&tone_id=` | app already knows a tone | TONE3000 verifies access, offers replacement if private/deleted; returned `tone_id` may differ |
| Device authorization (RFC 8628) | headless hardware, no browser | `POST /oauth/device_authorization`, user approves at `/activate`, poll `/oauth/token` |

Token response: `access_token`, `refresh_token`, `token_type: "bearer"`, `expires_in` (seconds), `scope`. Refresh tokens rotate — store every new one. Device codes expire in 600 s; minimum poll interval 5 s; `slow_down` adds 5 s.

### 2.3 Live-verified auth behaviour ✅

| Request | Status | Body |
|---|---|---|
| `GET /api/v1/user` (no header) | 401 | `{"error":"Missing or invalid Authorization header"}` |
| `GET /api/v1/user` (`Authorization: Bearer ` empty) | 401 | `{"error":"Missing or invalid Authorization header"}` |
| `GET /api/v1/user` (`Authorization: <raw token>`) | 401 | `{"error":"Missing or invalid Authorization header"}` |
| `GET /api/v1/user` (`Authorization: Bearer bogus`) | 401 | `{"error":"Unauthorized"}` |
| `GET /api/v1/user` (`Authorization: Bearer t3k_cs_<redacted>`) | 401 | `{"error":"Unauthorized"}` |

The secret-key format uses the same `Bearer` scheme; an invalid secret is reported as a generic `Unauthorized`, not a format error.

Device authorization validation ✅:

| Request | Status | Body |
|---|---|---|
| `POST /oauth/device_authorization` (empty) | 400 | `{"error":"invalid_request","error_description":"Missing client_id"}` |
| `POST ... client_id=probe-invalid` (form) | 400 | `{"error":"invalid_client","error_description":"Unknown client_id"}` |
| `POST ... {"client_id":"probe-invalid"}` (JSON) | 400 | `{"error":"invalid_client","error_description":"Unknown client_id"}` — JSON body accepted |
| `POST ... client_id=...&redirect_uri=...` | 400 | `invalid_request` — “device authorization grant is prompt-less basic OAuth only and does not accept: redirect_uri. Use /api/v1/oauth/authorize for select_tone / load_tone / load_model flows.” |
| `POST ... client_id=...&prompt=select_tone` | 400 | `invalid_request` — same message, naming `prompt` |
| `POST /oauth/token` (only `client_id`) | 400 | `{"error":"invalid_request","error_description":"Missing grant_type or client_id"}` |
| `POST /oauth/token` (`grant_type=authorization_code`, unknown client) | 400 | `{"error":"invalid_client","error_description":"Unknown client_id"}` |
| `POST /oauth/token` (`grant_type=refresh_token`, unknown client) | 400 | `{"error":"invalid_client","error_description":"Unknown client_id"}` |
| `POST /oauth/token` (`grant_type=urn:ietf:params:oauth:grant-type:device_code`, unknown client) | 400 | `{"error":"invalid_client","error_description":"Unknown client_id"}` |

⚠️ The device-flow error text names a **`load_model`** flow (`select_tone / load_tone / load_model`) that the main docs page does not document. Treat `prompt=load_model` as an undocumented/possibly-upcoming value.

Not verified (needs a real publishable key + user approval): successful authorize redirect, code exchange, refresh, device poll success, and whether the secret key can substitute for a user token on every endpoint.

---

## 3. Endpoint table

`Bearer` = user access token **or** account secret key (`t3k_cs_...`). “Public” = no auth required (✅ live-verified).

| Method | Path | Purpose | Auth | Pagination | `page_size` cap | Notes |
|---|---|---|---|---|---|---|
| GET | `/api/v1/user` | Current user profile | Bearer | — | — | 📄 |
| GET | `/api/v1/users` | Public user directory | Bearer | `page`, `page_size` | 10 | `sort`, `query` 📄 |
| GET | `/api/v1/tones/created` | Authenticated user's tones | Bearer | `page`, `page_size` | 100 | `gear`, `query` 📄 |
| GET | `/api/v1/tones/favorited` | Authenticated user's favorites | Bearer | `page`, `page_size` | 100 | `gear`, `query` 📄 |
| GET | `/api/v1/tones/downloaded` | User's downloads (deduped) | Bearer | `page`, `page_size` | 100 | `gear`, `query` 📄 |
| GET | `/api/v1/tones/search` | Search / browse catalog | Bearer | `page`, `page_size` | 25 | many filters; **heavily rate-limited** 📄 |
| GET | `/api/v1/tones/trending` | Top 10 trending | **Public** ✅ | not paginated | — | `gear`; ignores `page`/`page_size` ✅ |
| GET | `/api/v1/tones/latest` | 10 most recent | Bearer | not paginated | — | 📄 |
| GET | `/api/v1/tones/{id}` | Tone detail | Bearer | — | — | `architecture` 📄 |
| PUT | `/api/v1/tones/{id}/favorite` | Favorite (idempotent) | Bearer | — | — | 200 + `Favorite` 📄 |
| DELETE | `/api/v1/tones/{id}/favorite` | Unfavorite (idempotent) | Bearer | — | — | 204 no body 📄 |
| GET | `/api/v1/tones/{id}/download` | Zip download URL | Bearer + **approved partner only** | — | — | 403 for others; `filenames=name\|id` 📄 |
| GET | `/api/v1/models?tone_id=` | List a tone's models | Bearer | `page`, `page_size` | 300 | `architecture` 📄 |
| GET | `/api/v1/models/{id}` | Model detail | Bearer | — | — | 📄 |
| GET | `/api/v1/models/lookup?content_hash=` | Find models by content hash | Bearer | `page`, `page_size` | 25 | oldest first; empty list if no match 📄 |
| GET | `/api/v1/makes` | Makes by tone count | Bearer | `page`, `page_size` | 25 | `query`, `sort` 📄 |
| GET | `/api/v1/tags` | Tags by tone count | Bearer | `page`, `page_size` | 25 | `query`, `sort` 📄 |
| GET | `/api/v1/oauth/authorize` | Start OAuth redirect flow | `client_id` + PKCE | — | — | redirect, no JSON body ✅ (400 on bad params) |
| POST | `/api/v1/oauth/token` | Exchange / refresh / device poll | `client_id` | — | — | form-encoded ✅ (error paths) |
| POST | `/api/v1/oauth/device_authorization` | Request device code | `client_id` | — | — | form or JSON ✅ (error paths) |
| GET | `/api/files/models/{id}/{filename}` | Web app file route | signed redirect (no auth) ✅ | — | — | 302 → `files.tone3000.com` ✅ |

Routing behaviour ✅:

- `GET /api/v1/tones` → **405** (no collection route).
- `GET /api/v1/nope` → **404**, `content-type: text/html` (Next.js not-found page, not JSON).
- `GET /api/v1` → **404 HTML**.
- Unknown routes therefore cannot be detected by content type alone; check status + `content-type`.

---

## 4. Verified examples (live, this recon)

### 4.1 Public trending feed ✅

```bash
curl -sS 'https://www.tone3000.com/api/v1/tones/trending'
```

Headers:

```http
HTTP/2 200
access-control-allow-headers: Content-Type, Authorization
access-control-allow-methods: GET, OPTIONS
access-control-allow-origin: *
cache-control: public, max-age=0, must-revalidate
content-type: application/json
server: Vercel
x-matched-path: /api/v1/tones/trending
x-vercel-cache: MISS
```

Body (trimmed; the real response is 10 tones, ~18 KB):

```json
{
  "data": [
    {
      "id": 106343,
      "user_id": "c5b78330-855e-4f97-8691-ad0c2b49dfa7",
      "title": "Mesa Boogie Dual Rectifier Rev F - Red Channel + OD808 - High Gain",
      "description": "Get the full capture pack for this amp here: https://www.amalgamcaptures.com/nam/ ...",
      "created_at": "2026-10-30T10:36:15.468492+00:00",
      "updated_at": "2026-10-30T15:39:14.311249+00:00",
      "images": ["https://assets.tone3000.com/4c9ac054-f731-4dd9-961e-5b406823aee2.jpg"],
      "is_public": true,
      "links": ["https://amalgamcaptures.com/product/mes-drecf-txdi/", "https://youtu.be/LpeSzFyeAxI"],
      "license": "t3k",
      "models_count": 1,
      "favorites_count": 669,
      "downloads_count": 46255,
      "sizes": null,
      "published_at": "2026-10-30T15:39:11.73837+00:00",
      "a1_models_count": 0,
      "a2_models_count": 1,
      "custom_models_count": 0,
      "irs_count": 0,
      "makes": [{"id": 378, "name": "Maxon OD808"}, {"id": 102705, "name": "Mesa Boogie Dual Rectifier Rev F 1993"}],
      "tags": [{"id": 39, "name": "metal"}, {"id": 103, "name": "nam"}],
      "user": {
        "id": "c5b78330-855e-4f97-8691-ad0c2b49dfa7",
        "username": "amalgamaudio",
        "avatar_url": "https://assets.tone3000.com/d77bfn8wf0m.jpg",
        "display_name": "AmalgamAudio",
        "is_verified": true,
        "url": "https://www.tone3000.com/amalgamaudio"
      },
      "is_favorite": false,
      "url": "https://www.tone3000.com/tones/mesa-boogie-dual-rectifier-rev-f-red-channel-od808-high-gain-106343",
      "format": "nam",
      "gear": "amp-cab"
    }
  ]
}
```

Notes ✅:

- Envelope is `{"data": [...]}` — **not** the paginated envelope; trending is fixed at 10 items.
- `user_id` is a **UUID string**, while the docs' TypeScript interface says `int`. The nested `user.id` is also a UUID string. ⚠️
- `sizes` is `null` here even though the type is `Size[]`.
- `is_favorite` is `false` for anonymous callers (no user context).
- `page`, `page_size` are ignored (`?page=2&page_size=50` returns the same 10; `?page_size=1` also returns 10). ✅

Filtering behaviour ✅:

| Query | Status | Result |
|---|---|---|
| `?gear=amp-cab` | 200 | amp-cab tones |
| `?gear=full-rig` | 200 | same set as `amp-cab` (deprecated alias accepted; **no** `Deprecation`/`X-Tone3000-Deprecations` header on this endpoint) |
| `?gear=ir` | 200 | IR tones (accepted here, despite the docs saying `ir` is stripped from `gears` and inferred as `format=ir`) ⚠️ |
| `?gear=bogus` | 400 | `{"error":"gear must be one of: amp, amp-cab, pedal, outboard, cab, space, experimental"}` |
| `?platform=nam` | 200 | ignored / accepted; no deprecation header on trending |

### 4.2 Auth gate ✅

```bash
curl -sS -i 'https://www.tone3000.com/api/v1/user'
# HTTP/2 401
# {"error":"Missing or invalid Authorization header"}

curl -sS -i -H 'Authorization: Bearer bogus' 'https://www.tone3000.com/api/v1/user'
# HTTP/2 401
# {"error":"Unauthorized"}
```

All of these return 401 unauthenticated ✅: `/user`, `/users`, `/tones/search`, `/tones/latest`, `/tones/created`, `/tones/favorited`, `/tones/downloaded`, `/tones/{id}`, `/models`, `/models/{id}`, `/models/lookup`, `/makes`, `/tags`.

### 4.3 OAuth error paths ✅

See the tables in §2.3. Example:

```bash
curl -sS -X POST -H 'Content-Type: application/x-www-form-urlencoded' \
  --data 'client_id=probe-invalid' \
  'https://www.tone3000.com/api/v1/oauth/device_authorization'
# 400 {"error":"invalid_client","error_description":"Unknown client_id"}
```

`GET /api/v1/oauth/authorize` with missing or invalid params → **400 with an empty body** (no JSON error object). ✅

### 4.4 Model file delivery ✅

The public web app serves model files through a signed redirect (this is the delivery mechanism behind the API's `model_url`, but it is **not** an `/api/v1` route):

```bash
curl -sS -I 'https://www.tone3000.com/api/files/models/379978/7e9gmdzciqu_a2.nam'
```

```http
HTTP/2 302
cache-control: private, max-age=300
location: https://files.tone3000.com/7e9gmdzciqu_a2.nam?verify=1794696300-<signature>
server: Vercel
x-matched-path: /api/files/models/[id]/[filename]
```

Following the redirect:

```http
HTTP/2 200
content-type: application/json          # .nam is JSON
content-length: 301225
etag: "2c3e78842ded1d6d89e6e87fd5e616a4"
cache-control: public, max-age=300
access-control-allow-origin: *
server: cloudflare
```

Verified properties ✅:

- No `Authorization` header is required for this route; a bogus `Authorization` header does not change the result.
- The `verify=<timestamp>-<signature>` token is required: `https://files.tone3000.com/<file>` without it → **403**.
- A signed URL still returned 200 after 65 s; exact TTL unknown. Both redirect and file are cached `max-age=300`.
- Unknown model id → 404.
- IR example: `/api/files/models/901393/eff0929d-853e-4bf6-b10f-272d47e3dc81.wav` → 302 → `content-type: audio/wav`, `content-length: 73984`, and `cache-control: public, max-age=31536000, immutable` (IR is immutable; NAM was `max-age=300`).

⚠️ The v1 API's `model_url` value could not be observed (needs auth). It is documented as a “pre-built download URL” that requires the Bearer header; the web route above is the likely underlying pattern (`https://www.tone3000.com/api/files/models/{id}/{filename}`), but this is unconfirmed.

### 4.5 NAM file format ✅

One public A2 NAM file was downloaded (301,225 bytes, SHA-256 `329f688414be07d72718cc79f378fa5dbe96095f4dd74bf12fb5a8d961072639`) and parsed:

```json
{
  "version": "0.7.0",
  "architecture": "SlimmableContainer",
  "config": { "submodels": [ ... 2 entries ... ] },
  "weights": [],
  "sample_rate": 48000,
  "metadata": {
    "date": {"year": 2026, "month": 5, "day": 9, "hour": 4, "minute": 29, "second": 6},
    "loudness": -22.982362,
    "gain": 0,
    "name": "API 512c - left mic gain 5",
    "modeled_by": "tone3000",
    "training": {
      "data": {"checks": {"passed": true, "version": 3},
               "latency": {"manual": null,
                           "calibration": {"delays": [-13], "warnings": {...},
                                           "recommended": -14, "safety_factor": 1,
                                           "algorithm_version": 1}}},
      "settings": {"ignore_checks": false},
      "validation_esr": 0.00028815008505125926
    },
    "gear_make": "API 512c",
    "gear_model": "API 512c",
    "tone_type": "preamp",
    "gear_type": "studio",
    "t3k_id": 379978,
    "t3k_gear": "outboard",
    "weights_hash": "nam-content-v1:388d6d0ec112e422cb7d35303a9d5da14932c99f7ac1dc543fd69a4b7d3f7128"
  }
}
```

✅ Cross-check: `metadata.weights_hash` equals the `content_hash` published for model `379978` on the public tone page (`nam-content-v1:388d6d0e…`). This confirms the docs' statement that the API `content_hash` is the NAM `metadata.weights_hash`.

File formats ✅:

| Format | Extension | Content type | Notes |
|---|---|---|---|
| NAM (A1/A2/custom) | `.nam` | `application/json` | JSON document, version `0.7.0`, architecture e.g. `SlimmableContainer` |
| IR | `.wav` | `audio/wav` | immutable cache on the file host |
| Other documented formats | `aida-x`, `aa-snapshot`, `proteus` | — | no extensions observed; 📄 only |

---

## 5. Documented success shapes (authenticated — not live-verified)

These are the official request/response shapes. All require `Authorization: Bearer <token>` unless noted.

### 5.1 Paginated envelope 📄

```json
{ "data": [], "page": 1, "page_size": 10, "total": 0, "total_pages": 0 }
```

### 5.2 `GET /api/v1/tones/search` 📄

Params: `query`, `page`, `page_size` (max 25), `sort`, `gears`, `sizes`, `tags`, `makes`, `creators`, `format`, `architecture`, `calibrated`, `verified`.
Response: `PaginatedResponse<Tone[]>`.
⚠️ “Heavily rate-limited by default”; TONE3000 recommends the Select OAuth flow for browsing instead.

### 5.3 `GET /api/v1/tones/{id}` 📄

Param `architecture` (`1` | `2` | `custom`). Omitting it returns the legacy A1 + Custom count and excludes A2. Always returns `a1_models_count`, `a2_models_count`, `custom_models_count`. Response: `Tone`.

### 5.4 `GET /api/v1/models?tone_id=` 📄

Params `page`, `page_size` (max 300), `architecture`. Response `PaginatedResponse<Model[]>`, tone-page order (owner position, then newest first). `model_url` requires the Bearer header to fetch.

⚠️ Architecture default: omitting `architecture` returns A1 + Custom only; there is no “all” value. The Rust client documents a real trap: a tone reporting `models_count: 223` can return only 112 models on a bare call. Request each architecture the tone reports (`a1_models_count`, `a2_models_count`, `custom_models_count`) and concatenate.

### 5.5 `GET /api/v1/models/lookup?content_hash=` 📄

`content_hash` must be `nam-content-v1:` or `ir-content-v1:` + 64 hex; anything else → 400. Response `PaginatedResponse<Model[]>`, oldest first. No match → empty list, not 404. Only models readable by the caller (public / owned / favorited) are returned; all architectures included. For `.nam`, send `metadata.weights_hash`; for IR, `ir-content-v1:` + hex SHA-256 of `ir-content-v1\0` + WAV `data` chunk.

### 5.6 `GET /api/v1/tones/{id}/download` 📄 (partner-only)

Response `{ "url": "...", "expires_at": "ISO 8601", "filename": "My Tone.zip" }`. `url` is a temporary no-auth link, expires 1 hour after issue. For `nam` tones the zip contains A2 files only. Returns 403 for non-partner clients; 400 if the tone has no downloadable models.

### 5.7 `GET /api/v1/tones/trending` and `/latest` 📄 (trending is public ✅)

`{ "data": Tone[] }`, at most 10, not paginated. Trending: optional `gear`. Latest: no params; covers `nam` and `ir`.

---

## 6. Pagination

- **Scheme:** offset pagination with `page` (1-based) and `page_size`; envelope `{data, page, page_size, total, total_pages}`. There is **no cursor** scheme. 📄
- **Caps differ per endpoint** 📄:

| Endpoint | Default `page_size` | Max |
|---|---|---|
| `/users` | 10 | 10 |
| `/tones/search` | 10 | 25 |
| `/tones/created`, `/favorited`, `/downloaded` | 10 | 100 |
| `/models?tone_id=` | 10 | 300 |
| `/models/lookup` | 10 | 25 |
| `/makes`, `/tags` | 10 | 25 |
| `/tones/trending`, `/tones/latest` | — | not paginated |

- Exceeding a cap is unverified (needs auth for most; trending ignores pagination entirely ✅).
- `total_pages` enables a full walk; the Rust client exposes `has_next()`.

---

## 7. Metadata field reference

### 7.1 `Tone` (docs 📄; field set confirmed live on trending ✅)

| Field | Docs type | Live trending | Notes |
|---|---|---|---|
| `id` | number | ✅ int | |
| `user_id` | int | ✅ **UUID string** | ⚠️ type mismatch vs docs |
| `user` | EmbeddedUser | ✅ | |
| `created_at` | str | ✅ ISO 8601, microsecond + `+00:00` | |
| `updated_at` | str | ✅ | |
| `published_at` | str \| null | ✅ | null if never published |
| `title` | str | ✅ | |
| `description` | str \| null | ✅ | |
| `gear` | Gear | ✅ | |
| `images` | str[] \| null | ✅ | |
| `is_public` | boolean \| null | ✅ | |
| `links` | str[] \| null | ✅ | |
| `format` | Format | ✅ | docs say `platform` is deprecated/absent |
| `license` | License | ✅ | |
| `sizes` | Size[] | ✅ `null` observed | |
| `makes` | Make[] | ✅ | |
| `tags` | Tag[] | ✅ | |
| `models_count` | number | ✅ | cross-architecture total (see §5.4 warning) |
| `a1_models_count` | number | ✅ | |
| `a2_models_count` | number | ✅ | |
| `irs_count` | number | ✅ | |
| `custom_models_count` | number | ✅ | |
| `downloads_count` | number | ✅ | |
| `favorites_count` | number | ✅ | |
| `is_favorite` | boolean | ✅ | false when unauthenticated |
| `url` | str | ✅ | public tone page URL |

### 7.2 `EmbeddedUser` 📄 (confirmed live ✅)

`id` (UUID string live ⚠️), `username`, `display_name` (only set for verified creators), `is_verified`, `avatar_url`, `url`.

### 7.3 `Model` 📄 (API docs) vs public page ⚠️

| Field | API docs | Public tone page ✅ | Notes |
|---|---|---|---|
| `id` | number | ✅ | |
| `name` | str | ✅ | |
| `model_url` | str | ✅ `r2://models/...` | internal Vercel R2 reference on the site; API value unverified |
| `created_at` | str | ✅ | |
| `updated_at` | str | ❌ absent | API-only? |
| `user_id` | int | ✅ UUID string | |
| `size` | Size | ✅ `null` observed | |
| `tone_id` | number | ❌ absent | API-only? |
| `architecture_version` | Architecture \| null | ✅ `"2"` | null for non-NAM |
| `content_hash` | str \| null | ✅ | equals NAM `metadata.weights_hash` |
| `metadata` | Record \| null | ❌ absent | API-only? partner capture metadata |
| `training_id` | — | ✅ | extra site field |
| `input_level` | — | ✅ `null` | extra site field |
| `output_level` | — | ✅ `null` | extra site field |
| `position` | — | ✅ `null` | extra site field |
| `training` | — | ✅ object ref | extra site field |

⚠️ The public site's model payload is not the API's `Model` shape. Do not treat site fields as API fields.

### 7.4 Other types 📄

- `User` = EmbeddedUser + `bio`, `links`, `created_at`, `updated_at`.
- `PublicUser` = `id`, `username`, `display_name`, `is_verified`, `bio`, `links`, `avatar_url`, `downloads_count`, `favorites_count`, `models_count`, `tones_count`, `url`.
- `Make` = `id`, `name`; `Tag` = `id`, `name`.
- `PublicMake` / `PublicTag` = `id`, `name`, `tones_count`, `url`.
- `Favorite` = `id`, `tone_id`, `user_id`, `created_at`.
- `PaginatedResponse<T>` = `data`, `page`, `page_size`, `total`, `total_pages`.
- `Session` = `access_token`, `refresh_token`, `expires_in`, `token_type`, `scope`.
- `ToneDownload` = `url`, `expires_at`, `filename`.

### 7.5 Enums 📄

- **Gear:** `amp`, `amp-cab`, `pedal`, `outboard`, `cab`, `space`, `experimental` (+ deprecated `full-rig` → `amp-cab`, `ir` → use `format=ir`).
- **Architecture:** `1` (A1), `2` (A2), `custom`. Omitting → A1 + Custom (excludes A2).
- **Format:** `nam`, `ir`, `aida-x`, `aa-snapshot`, `proteus` (formerly `platform`).
- **License:** `t3k`, `cc-by`, `cc-by-sa`, `cc-by-nc`, `cc-by-nc-sa`, `cc-by-nd`, `cc-by-nc-nd`, `cco`.
- **Size:** `standard`, `lite`, `feather`, `nano`, `custom`.
- **TonesSort:** `best-match`, `newest`, `oldest`, `trending`, `downloads-all-time`.
- **UsersSort:** `tones`, `downloads`, `favorites`, `models`.
- **TaxonomySort:** `tones`, `name`.

---

## 8. Rate limits and quotas

- **Default: 100 requests per minute.** 📄 (docs + Rust client). Production integrations should email `support@tone3000.com`.
- **Search is “heavily rate-limited by default”** 📄; the docs explicitly recommend the Select OAuth flow for browsing/search instead of `/tones/search`.
- **No rate-limit headers observed** ✅ on any 200/400/401 response in this recon (`x-ratelimit-*`, `retry-after`, `X-Tone3000-*` all absent). The 429 threshold and any `Retry-After` header are **not live-verified** — deliberately not probed, to avoid hammering a shared-IP quota.
- Reference clients handle 429 with a `retry_after` value 📄; honour it and back off.
- Device flow: `interval` 5 s minimum; `slow_down` → add 5 s; codes expire in 600 s. 📄
- Free tier is further restricted to bounded list endpoints (see §10). 📄

---

## 9. Caching / redistribution / ToS constraints

From `https://www.tone3000.com/api/terms` (effective 2026-07-07) 📄:

- **No bulk download, scrape, mirror, or cache of the catalog.** Only download models when a user requests them.
- **Per-user, authenticated delivery.** Do not preload catalog content onto devices or bundle it with a product without written permission.
- **No redistribution / republishing** of catalog content outside your integration — including hosting tones on your own servers or sharing them on other platforms.
- **Preserve creator names, tone metadata, and license information.** Do not strip or alter attribution.
- **Respect creator licenses** (`t3k`, CC BY, CC BY-NC, …). The API grants no ownership, only delivery to the authenticated user who requested the content.
- **No white-labeling.** Browsing, search, tone delivery, capture and training must be presented as TONE3000.
- **No proxying/pooling user accounts** through accounts you control; all access is tied to the end user's TONE3000 account.
- **Branding:** identify TONE3000 with “Powered by TONE3000”, link to `https://www.tone3000.com`, follow the logo rules (full TONE3000 logo before the compact T3K mark).
- **Commercial use:** free only for non-commercial products (free/open software or hardware, DIY pedals, research, community tools). Free tier may use **only** the OAuth prompt flows (`select_tone`, `load_tone`) and the bounded list endpoints (`favorited`, `downloaded`, `created`, `trending`, `latest`). Paid products or products accompanying a paid product need a commercial agreement and sign-off.
- The **NAM format itself is open source** and free to implement; the terms govern API/catalog access, not the format.

⚠️ **Direct conflict to flag:** a “local index of all browsable NAM models” (as planned on the parent card) requires bulk listing/caching of the catalog, which the free-tier terms prohibit and which is only available to commercial/approved partners. See §12.

---

## 10. Deprecations 📄

Deprecated inputs still work but are normalised; responses carry `Deprecation: true`, `Link: <…>; rel="deprecation"`, and `X-Tone3000-Deprecations: <kinds>`.

| Deprecated | Replacement | Endpoint |
|---|---|---|
| `?platform=…` | `?format=…` | all format filters |
| `?gear=…` | `?gears=…` | search, Select, Load Tone |
| `gears=ir` | `format=ir` | search, Select, Load Tone |
| `gears=full-rig` | `gears=amp-cab` | search, Select, Load Tone |

✅ On the public `/tones/trending`, `gear=full-rig` and `gear=ir` both returned 200 with **no** deprecation headers — the deprecation machinery is documented for search/Select/Load Tone, not necessarily for trending.

---

## 11. Unknowns and risks

1. **No live authenticated verification.** No publishable key, secret key, or user token was available. All success shapes in §5 are documented, not observed. The exact `model_url` value, the paginated envelope, and the `Model`/`User` response bodies remain unverified.
2. **Free-tier vs index conflict.** The parent plan's “refreshable local index of all browsable NAM models” requires bulk catalog access/caching, which the API ToS prohibits for free-tier integrations and which is reserved for commercial/approved partners. The integration design must either (a) stay within the free tier (Select/Load Tone + bounded lists, download on user request, no catalog cache), or (b) obtain a commercial agreement. This is a decision for the user, not something code can route around.
3. **`architecture` default trap.** Omitting `architecture` returns A1 + Custom and silently excludes A2. A tone's `models_count` is cross-architecture, so a naive list screen can show “223 models” and list 112. Always pass `architecture=2` (or loop the reported per-architecture counts).
4. **Docs vs live type drift.** `user_id` is a UUID string live but `int` in the docs; `sizes` is `null` on trending; the public site still emits `platform` while the API emits `format`; the public site exposes extra fields (`is_deleted`, `trending_score`, `has_a1_models`, `has_a2_models`, `has_model_with_url`, `has_model_with_calibration`, `visible_comments_count`) that the API `Tone` type does not document.
5. **Undocumented `load_model` prompt** named in the device-flow error text.
6. **Rate-limit response shape unknown.** No 429 was provoked; whether `Retry-After` is a header or body field is unverified.
7. **`page_size` cap enforcement unknown** (clamp vs 400) for authenticated endpoints.
8. **Tone zip download is partner-only** (403). Per-model `model_url` download is the supported path for normal integrations.
9. **Secret key is not a user token.** Using `t3k_cs_…` on a server to serve many end users likely violates the per-user delivery / no-proxying clauses. For a pedal, the device flow (or a per-user token) is the intended route; keep any secret key server-side only, if used at all.
10. **API is v1 and explicitly beta** — endpoints and data structures may change.
11. **Signed file URLs** are short-lived-ish (worked after 65 s; exact TTL unknown) and require the `verify` token; do not persist them. The NAM file response is cached `max-age=300`, IR `immutable` for 1 year.
12. **Content-hash recipes** are documented for NAM (`metadata.weights_hash`) and IR (`ir-content-v1:` + SHA-256 of `ir-content-v1\0` + WAV `data` chunk); only the NAM side was cross-checked ✅.

---

## 12. How to finish live verification (when credentials exist)

Set names only — never commit values:

```bash
export T3K_PUB_KEY='t3k_pub_...'        # publishable / client_id
export T3K_ACCESS_TOKEN='...'           # user access token (or secret key, server-side only)
```

Then, for each authenticated endpoint:

```bash
curl -sS -H "Authorization: Bearer $T3K_ACCESS_TOKEN" \
  'https://www.tone3000.com/api/v1/tones/search?query=plexi&page=1&page_size=25&architecture=2'
curl -sS -H "Authorization: Bearer $T3K_ACCESS_TOKEN" \
  'https://www.tone3000.com/api/v1/models?tone_id=<id>&architecture=2&page=1&page_size=300'
curl -sS -H "Authorization: Bearer $T3K_ACCESS_TOKEN" \
  'https://www.tone3000.com/api/v1/models/<id>'
curl -sS -H "Authorization: Bearer $T3K_ACCESS_TOKEN" \
  'https://www.tone3000.com/api/v1/models/lookup?content_hash=nam-content-v1:<64hex>'
```

Capture and compare against §5. In particular, record the real `model_url` string and the paginated envelope.

---

## 13. Evidence index (this workspace)

| File | Contents |
|---|---|
| `probes/trending_clean.json` | full live 200 response, `/api/v1/tones/trending` ✅ |
| `probes/trending_full.txt`, `trending_gear_*`, `trending_page*` | headers + bodies for trending variants ✅ |
| `probes/user*.txt`, `tones_search*.txt`, `tone_detail.txt`, `models_list.txt`, `model_detail.txt`, `users.txt`, `makes.txt`, `tags.txt`, `latest.txt`, `favorited.txt`, `downloaded.txt`, `created.txt`, `lookup_hash.txt` | live 401 auth-gate responses ✅ |
| `probes/device_auth_*.txt`, `token_*.txt`, `devjson.*`, `devredir.*`, `devprompt.*`, `token_nogrant.*` | live OAuth error responses ✅ |
| `probes/options_preflight.txt` | CORS preflight 204 ✅ |
| `probes/unknown_endpoint.txt`, `api_root.txt` | 404 HTML behaviour ✅ |
| `probes/tone_page.html`, `ir_tone_page.html` | public tone pages (RSC payloads; model objects, `model_url`, file extensions) ✅ |
| `probes/embed_player.html` | public embed player page ✅ |
| `evidence/model_379978_a2.nam` | one downloaded public A2 NAM file (301,225 bytes, SHA-256 `329f6884…`) ✅ |

No credentials, tokens, or secret values are stored in this workspace or in this document.
