//! Downloads: one shared HTTP client, files checked against their SHA-1, many at once.

use anyhow::{bail, Context, Result};
use futures::stream::{self, StreamExt};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

/// How the launcher names itself to the services it calls.
pub const USER_AGENT: &str = concat!(
    "MistinnLauncher/",
    env!("CARGO_PKG_VERSION"),
    " (+https://moriteahouse.shizune.life)"
);
/// How many files are downloaded at once.
const PARALLEL: usize = 16;
/// How many times a download is tried before giving up.
const RETRIES: usize = 3;

/// The one HTTP client every request goes through.
pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("http client")
    })
}

/// Fetches and parses a JSON document.
pub async fn json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let response = client().get(url).send().await.with_context(|| format!("GET {url}"))?;
    if !response.status().is_success() {
        bail!("GET {url}: HTTP {}", response.status());
    }
    response.json::<T>().await.with_context(|| format!("JSON from {url}"))
}

/// One file to fetch. `sha1` is checked when given; an existing file that matches is kept.
#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
}

/// The SHA-1 of a file, in hex; `None` when it cannot be read.
pub fn sha1_of(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(hex::encode(Sha1::digest(&bytes)))
}

/// Whether the file is already there (and matches its SHA-1, when one is known).
fn is_current(item: &Download) -> bool {
    match (&item.sha1, item.path.exists()) {
        (_, false) => false,
        (Some(expected), true) => sha1_of(&item.path).is_some_and(|h| h.eq_ignore_ascii_case(expected)),
        (None, true) => true,
    }
}

/// Downloads one file unless it is already current, retrying a few times.
async fn fetch_one(item: &Download) -> Result<()> {
    if is_current(item) {
        return Ok(());
    }
    let mut last = None;
    for attempt in 0..RETRIES {
        match fetch_attempt(item).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1))).await;
            }
        }
    }
    Err(last.unwrap())
}

/// One try at a download: checked against its SHA-1 and written in place only when whole.
async fn fetch_attempt(item: &Download) -> Result<()> {
    let response = client().get(&item.url).send().await.with_context(|| format!("GET {}", item.url))?;
    if !response.status().is_success() {
        bail!("GET {}: HTTP {}", item.url, response.status());
    }
    let bytes = response.bytes().await?;
    if let Some(expected) = &item.sha1 {
        let got = hex::encode(Sha1::digest(&bytes));
        if !got.eq_ignore_ascii_case(expected) {
            bail!("{}: checksum {got} does not match {expected}", item.url);
        }
    }
    if let Some(parent) = item.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // write beside the target, then move into place, so a broken download never looks finished
    let mut partial = item.path.clone().into_os_string();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    tokio::fs::write(&partial, &bytes).await?;
    tokio::fs::rename(&partial, &item.path).await?;
    Ok(())
}

/// Fetches every item, `PARALLEL` at a time, reporting (done, total) as it goes.
pub async fn fetch_all(items: Vec<Download>, progress: impl Fn(usize, usize) + Send + Sync + 'static) -> Result<()> {
    let total = items.len();
    let done = Arc::new(AtomicUsize::new(0));
    let progress = Arc::new(progress);
    progress(0, total);
    let results: Vec<Result<()>> = stream::iter(items)
        .map(|item| {
            let done = done.clone();
            let progress = progress.clone();
            async move {
                let result = fetch_one(&item).await;
                let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                progress(n, total);
                result
            }
        })
        .buffer_unordered(PARALLEL)
        .collect()
        .await;
    let failures: Vec<String> = results
        .into_iter()
        .filter_map(std::result::Result::err)
        .map(|e| format!("{e:#}"))
        .collect();
    if !failures.is_empty() {
        bail!("{} of {total} downloads failed; first: {}", failures.len(), failures[0]);
    }
    Ok(())
}
