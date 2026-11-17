# TONE3000 client & local model index — usage

Status: **implemented** (`tone3000/` crate). Contracts: `docs/MODEL-BROWSE-AND-CACHE.md`
(storage, record schema, refresh semantics) and `docs/TONE3000-API-RECON.md` (endpoints,
auth, rate limits, terms).

The crate is deliberately standalone: it has no dependency on the audio engine and can be
used from the CLI, the engine, or a test.

---

## 1. Layout

| Module | Contents |
|---|---|
| `auth` | `Credentials::from_env()` (`T3K_ACCESS_TOKEN`, then `T3K_SECRET_KEY`); `Secret`, whose `Debug`/`Display` are redacted |
| `api` | Typed response shapes: `Tone`, `Model`, `Paginated<T>`, `ToneList`, `Gear`, `Format`, `Architecture` |
| `client` | `Client`, `Transport`, `ReqwestTransport`, `RetryPolicy`, `Error`/`TransportError`, `fetch_all_pages` |
| `index` | `Index`/`ModelRecord`/`Metadata`/`LocalState`, JSON-Schema validation, atomic writes, `merge`, `IndexBuilder` |
| `query` | `listModels` search/filter/sort/facets/paging over a loaded `Index` |
| `paths` | `PEDAL_HOME` / `PEDAL_MODELS` resolution and the `index/` + `models/` layout |
| `bin/t3k-refresh-index` | The refresh CLI |

## 2. Credentials

```sh
export T3K_ACCESS_TOKEN=t3k_...     # per-user OAuth access token (preferred)
# or
export T3K_SECRET_KEY=t3k_cs_...    # account secret key (server-side only)
```

* `T3K_ACCESS_TOKEN` wins when both are set; empty values count as unset.
* The value is only ever used to build an `Authorization: Bearer …` header in memory.
* It is never written into a URL, an error message, a log line, or a serialized index
  record. `Secret`'s `Debug` and `Display` render as `***`, so `#[derive(Debug)]` on a
  struct holding credentials cannot leak it.
* No credential value appears anywhere in the source tree — only the variable *names*.

## 3. Client

```rust,ignore
use pedal_tone3000::auth::Credentials;
use pedal_tone3000::client::{Client, RetryPolicy, ReqwestTransport, DEFAULT_BASE_URL};
use std::sync::Arc;

let credentials = Credentials::from_env();           // None when unset
let retry = RetryPolicy {
    min_interval: Duration::from_millis(600),        // stay under ~100 req/min
    ..RetryPolicy::default()
};
let client = Client::new(
    Arc::new(ReqwestTransport::new(Duration::from_secs(30))?),
    DEFAULT_BASE_URL,
    credentials,
    retry,
);
```

* `get_raw(path, query)` → one `Response`, with auth, timeout and retry.
* `get_json<T>(path, query)` → parsed JSON, or `Error::Malformed`.
* `get_paginated::<T>(path, query, page, page_size)` → one page; rejects a `page_size` of 0
  and an envelope that echoes the wrong page.
* `fetch_all_pages::<T>(path, query, page_size)` → walks `total_pages` in order, with a hard
  cap (100 000 pages) so a broken envelope cannot loop forever.
* `get_tone_feed(path, query)` → the non-paginated `{data: […]}` feeds.

### Errors

| Variant | Meaning | Retried? |
|---|---|---|
| `Error::Auth { status, message }` | HTTP 401/403 | never |
| `Error::RateLimited { attempts, retry_after }` | HTTP 429 after the budget | until the budget |
| `Error::Malformed { url, reason }` | bad JSON, empty body, impossible envelope | never |
| `Error::Http { status, url, message }` | other non-retryable status; 5xx after retries | 5xx only |
| `Error::RetryExhausted { url, attempts, source }` | transport/timeout failures after the budget | until the budget |
| `Error::Timeout`, `Error::Transport`, `Error::Config` | transport-level and setup failures | — |

`RetryPolicy::delay_for` is exponential (500 ms base, ×2, capped at 30 s) with ±25 % jitter;
`Retry-After` (delta-seconds or HTTP date, from the header or a `retry_after` / `retryAfter` /
`retry_after_seconds` body field) always wins when longer. `min_interval` throttles request
starts so a full refresh stays under the documented ~100 req/min.

## 4. Index refresh

```rust,ignore
use pedal_tone3000::index::{IndexBuilder, RefreshConfig, Source};
use pedal_tone3000::paths::Paths;

let paths = Paths::from_env();
let outcome = IndexBuilder::new(&client, RefreshConfig::default())
    .refresh(&paths)
    .await?;
// outcome.generated_at, .count, .tones, .skipped
```

* `Source::FreeFeeds` (default) walks `tones/trending` + `tones/latest`. Free-tier permitted,
  but `latest` needs a Bearer credential.
* `Source::Catalog` walks all of `tones/search`. The TONE3000 free-tier terms forbid bulk
  catalog listing/caching, so `RefreshConfig::catalog_agreement` must be explicitly `true`;
  the builder refuses before sending any request otherwise.
* Refresh is **all-or-nothing**: every page is fetched, every record is built, the merged
  index is validated, and only then is `models.json` atomically replaced (`tmp` → `bak` →
  rename). A failure leaves the previous index and its `.bak` untouched.
* `merge` carries `local` / `local_path` forward from the existing index by `id`, so a
  refresh can never orphan a download, and marks `local.stale` when both `content_hash`
  values are present and differ.

### CLI

```sh
cargo run -p pedal-tone3000 --bin t3k-refresh-index
cargo run -p pedal-tone3000 --bin t3k-refresh-index -- --show-status
cargo run -p pedal-tone3000 --bin t3k-refresh-index -- --source catalog \
    --i-have-a-commercial-agreement
```

`--show-status` prints the resolved base URL, source, **which kind** of credential was loaded
(never its value), and the index/models paths. Other flags: `--base-url`, `--min-interval-ms`,
`--page-size` (cap 25), `--models-page-size` (cap 300).

Exit codes: `0` refreshed, `1` refresh failed (old index kept), `2` usage/configuration error.

## 5. Validation

`docs/model-index.schema.json` (draft 2020-12) is embedded with `include_str!` and compiled
once. Every index is validated before it is written and after it is loaded, with
`format: uri` / `date-time` treated as assertions. The schema cannot express cross-field
equality, so `validate_index` additionally enforces `count == models.len()`.

`docs/example-models.json` is the known-good worked example and is validated in tests.

## 6. Querying

`query::list_models(&index, &query)` implements `listModels`: diacritic-folded substring
search (every token must match at least one weighted field), multi-select facets (OR within,
AND across, each facet counted with its own filter skipped), the eight frozen sort orders
(stable, ties broken by `id`), and paging applied after faceting. `Query::from_value` parses
the frozen camelCase wire payload and rejects unknown enum values.

## 7. Tests

```sh
cargo test -p pedal-tone3000          # unit tests, no network
```

Coverage includes: multi-page pagination and envelope validation, the page cap, retry/backoff
growth and capping, `Retry-After` (header + body), every `Error` variant, credential
redaction in headers/URLs/errors, kind/variant derivation, schema validation and its
rejections, merge/staleness, atomic all-or-nothing refresh, and the catalog agreement gate.
All HTTP is a scripted in-memory `Transport`; nothing reaches the network.

## 8. Terms reminder

The free tier permits the OAuth prompt flows and the bounded list endpoints
(`favorited`, `downloaded`, `created`, `trending`, `latest`). A full catalog index cache
requires a commercial/approved-partner agreement — see
`docs/TONE3000-API-RECON.md` §9. Downloads are per-model and user-initiated; the refresh
never downloads model files.
