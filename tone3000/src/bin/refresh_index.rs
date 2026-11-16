//! `t3k-refresh-index` — rebuild the local model index from TONE3000.
//!
//! Credentials come from the environment (`T3K_ACCESS_TOKEN`, or
//! `T3K_SECRET_KEY`) and are never echoed. Set `PEDAL_HOME` to choose where the
//! index is written; otherwise [`pedal_tone3000::paths`] picks the platform
//! default (or the dev checkout).
//!
//! ```sh
//! export T3K_ACCESS_TOKEN=t3k_...        # user access token preferred
//! cargo run -p pedal-tone3000 --bin t3k-refresh-index
//! # → Refreshed 1204 records from 96 tones into /Users/…/Library/Application Support/app.pedal/index/models.json
//! ```
//!
//! `--source catalog` walks the whole `/tones/search` catalog. The TONE3000
//! free tier only permits the bounded feeds (`trending`, `latest`), so a
//! catalog walk additionally requires `--i-have-a-commercial-agreement`; without
//! it the command refuses before sending any request.
//!
//! Diagnostics go to stderr, the result line to stdout. Nothing but status
//! codes, counts, paths and server-provided messages are printed.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use pedal_tone3000::auth::{AuthKind, Credentials};
use pedal_tone3000::client::{Client, DEFAULT_BASE_URL, RetryPolicy};
use pedal_tone3000::index::{IndexBuilder, RefreshConfig, Source};
use pedal_tone3000::paths::Paths;

const USAGE: &str = "\
t3k-refresh-index — refresh the local TONE3000 model index

Usage:
  t3k-refresh-index [options]

Options:
  --source <feeds|catalog>   Which listing to index (default: feeds)
  --i-have-a-commercial-agreement
                             Acknowledge a commercial/approved-partner agreement,
                             required for --source catalog
  --base-url <url>           API base URL (default: https://www.tone3000.com/api/v1)
  --min-interval-ms <ms>     Minimum gap between requests (default: 600)
  --page-size <n>            Page size for catalog search (default: 25, cap 25)
  --models-page-size <n>     Page size for tone models (default: 300, cap 300)
  --show-status              Print resolved configuration, then exit
  -h, --help                 Show this help

Environment:
  T3K_ACCESS_TOKEN   Per-user OAuth access token (preferred credential)
  T3K_SECRET_KEY     Account secret key (server-side use only)
  PEDAL_HOME         Where index/models.json lives
  PEDAL_MODELS       Override the downloaded-models directory

Exit codes: 0 ok, 2 usage error, 1 refresh failed.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("t3k-refresh-index: {message}");
            ExitCode::from(2)
        }
    }
}

struct Options {
    source: Source,
    catalog_agreement: bool,
    base_url: String,
    min_interval_ms: u64,
    search_page_size: u32,
    models_page_size: u32,
    show_status: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            source: Source::FreeFeeds,
            catalog_agreement: false,
            base_url: DEFAULT_BASE_URL.to_string(),
            // ~100 requests/min is the documented budget; stay under it.
            min_interval_ms: 600,
            search_page_size: 25,
            models_page_size: 300,
            show_status: false,
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    let mut options = Options::default();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "-h" | "--help" | "help" => {
                print!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            "--source" => {
                let value = take_value(&args, &mut index, arg)?;
                options.source = match value.as_str() {
                    "feeds" => Source::FreeFeeds,
                    "catalog" => Source::Catalog,
                    other => {
                        return Err(format!(
                            "unknown --source '{other}' (expected 'feeds' or 'catalog')"
                        ))
                    }
                };
            }
            "--i-have-a-commercial-agreement" => options.catalog_agreement = true,
            "--base-url" => {
                options.base_url = take_value(&args, &mut index, arg)?;
            }
            "--min-interval-ms" => {
                let value = take_value(&args, &mut index, arg)?;
                options.min_interval_ms = value
                    .parse()
                    .map_err(|_| format!("--min-interval-ms must be a number, got '{value}'"))?;
            }
            "--page-size" => {
                let value = take_value(&args, &mut index, arg)?;
                options.search_page_size = clamp_page_size(
                    value.parse::<u32>().map_err(|_| {
                        format!("--page-size must be a number, got '{value}'")
                    })?,
                    25,
                );
            }
            "--models-page-size" => {
                let value = take_value(&args, &mut index, arg)?;
                options.models_page_size = clamp_page_size(
                    value.parse::<u32>().map_err(|_| {
                        format!("--models-page-size must be a number, got '{value}'")
                    })?,
                    300,
                );
            }
            "--show-status" => options.show_status = true,
            other => return Err(format!("unknown argument '{other}'\n\n{USAGE}")),
        }
        index += 1;
    }

    if options.source.is_catalog() && !options.catalog_agreement {
        return Err(
            "--source catalog needs a commercial TONE3000 agreement (the free-tier terms \
             forbid bulk catalog listing and caching); pass \
             --i-have-a-commercial-agreement once that agreement is in place, or use \
             --source feeds"
                .to_string(),
        );
    }

    let credentials = Credentials::from_env();
    let paths = Paths::from_env();

    if options.show_status {
        print_status(&options, credentials.as_ref(), &paths);
        return Ok(ExitCode::SUCCESS);
    }

    // Fail before doing any work when an authenticated listing is requested
    // without a credential, so no partial index can be written.
    match options.source {
        Source::FreeFeeds => {}
        Source::Catalog => {
            if credentials.is_none() {
                return Err(
                    "--source catalog needs a credential: set T3K_ACCESS_TOKEN (or \
                     T3K_SECRET_KEY); see docs/TONE3000-API-RECON.md §2"
                        .to_string(),
                );
            }
        }
    }

    let retry = RetryPolicy {
        max_attempts: 4,
        initial_delay: Duration::from_millis(500),
        max_delay: Duration::from_secs(30),
        jitter: true,
        min_interval: Duration::from_millis(options.min_interval_ms),
    };
    let client = Client::new(
        Arc::new(
            pedal_tone3000::client::ReqwestTransport::new(
                pedal_tone3000::client::DEFAULT_TIMEOUT,
            )
            .map_err(|e| format!("could not build the HTTP client: {e}"))?,
        ),
        options.base_url.clone(),
        credentials,
        retry,
    );

    let config = RefreshConfig {
        source: options.source,
        catalog_agreement: options.catalog_agreement,
        search_page_size: options.search_page_size,
        models_page_size: options.models_page_size,
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("could not start the async runtime: {e}"))?;

    let outcome = runtime.block_on(async {
        IndexBuilder::new(&client, config).refresh(&paths).await
    });

    match outcome {
        Ok(outcome) => {
            println!(
                "Refreshed {} record(s) from {} tone(s) into {} (generated {})",
                outcome.count, outcome.tones, paths.index_file.display(), outcome.generated_at
            );
            if outcome.skipped > 0 {
                eprintln!("skipped {} unindexable model(s)", outcome.skipped);
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            eprintln!("refresh failed: {error}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

/// The API caps `page_size` per endpoint; exceed it and the request is rejected.
fn clamp_page_size(value: u32, cap: u32) -> u32 {
    if value == 0 {
        1
    } else {
        value.min(cap)
    }
}

fn print_status(options: &Options, credentials: Option<&Credentials>, paths: &Paths) {
    println!("base_url:        {}", options.base_url);
    println!("source:          {}", if options.source.is_catalog() { "catalog" } else { "feeds" });
    println!("credential:      {}", describe_credential(credentials));
    println!("pedal_home:      {}", paths.home.display());
    println!("index_file:      {}", paths.index_file.display());
    println!("models_dir:      {}", paths.models.display());
    println!("min_interval_ms: {}", options.min_interval_ms);
    println!("search_page_size:{}", options.search_page_size);
    println!("models_page_size:{}", options.models_page_size);
}

/// Never print a credential value — only which kind was loaded, or "none".
fn describe_credential(credentials: Option<&Credentials>) -> &'static str {
    match credentials.map(Credentials::kind) {
        Some(AuthKind::AccessToken) => "access token (T3K_ACCESS_TOKEN)",
        Some(AuthKind::SecretKey) => "secret key (T3K_SECRET_KEY)",
        None => "none",
    }
}
