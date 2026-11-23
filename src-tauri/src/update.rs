//! Updates of the launcher itself. Each released build knows the release it was made from: its tag
//! and the source commit it was built at, compiled in by the release workflow. At start the launcher
//! asks GitHub for its latest release and compares the two; when the source differs, the window
//! offers the update quietly, and the update window downloads the new build, checks it against the
//! SHA-256 GitHub records for it, swaps it in for the running one and starts it.
//!
//! A build made outside the release workflow knows no release and never offers an update. Only
//! releases of this repository are accepted, by exact tag form, file name and download address.

use crate::net;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The repository whose releases update the launcher.
const REPO: &str = "keeiv/mistinn-launcher";
/// Every accepted download starts with this, followed by the release's tag.
const DOWNLOAD_PREFIX: &str = "https://github.com/keeiv/mistinn-launcher/releases/download/";
/// How long the check at start may take before it is given up quietly.
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The release this build was made from, set by the release workflow; `None` for any other build.
pub const BUILD_TAG: Option<&str> = option_env!("MISTINN_RELEASE_TAG");
/// The source commit this build was made from, set by the release workflow.
pub const BUILD_COMMIT: Option<&str> = option_env!("MISTINN_BUILD_COMMIT");

/// One downloadable build in a release, with the SHA-256 GitHub records for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// A newer release than the running build, as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Available {
    /// The release's tag, such as `v0.1.1-mod.0.1.0`.
    pub tag: String,
    /// The release's title.
    pub name: String,
    /// The source commit the release was built at.
    pub commit: String,
    /// The running build's tag and commit.
    pub current_tag: String,
    pub current_commit: String,
    /// The build that replaces the running one in place.
    pub portable: Asset,
    /// The installer, for when the running one cannot be replaced.
    pub installer: Option<Asset>,
}

/// A release as GitHub's API describes it, as far as the launcher reads it.
#[derive(Debug, Deserialize)]
pub struct Release {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    target_commitish: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

/// One file of a release as GitHub's API describes it.
#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
    size: u64,
}

/// Whether `tag` has the launcher's release form, `v<launcher>-mod.<mod>`, each a plain x.y.z.
pub fn valid_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else { return false };
    let Some((launcher, module)) = rest.split_once("-mod.") else {
        return false;
    };
    [launcher, module].iter().all(|version| {
        let parts: Vec<&str> = version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Whether `text` is a full commit id: 40 lowercase hex digits.
fn valid_commit(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The release's file whose name ends with `suffix`, accepted only with the expected name, address
/// and a SHA-256 digest.
fn asset(release: &Release, suffix: &str) -> Option<Asset> {
    let found = release
        .assets
        .iter()
        .find(|a| a.name.starts_with("MistinnLauncher_") && a.name.ends_with(suffix))?;
    let expected_url = format!("{DOWNLOAD_PREFIX}{}/{}", release.tag_name, found.name);
    if found.browser_download_url != expected_url || found.name.contains(['/', '\\']) {
        return None;
    }
    let sha256 = found.digest.as_deref()?.strip_prefix("sha256:")?.to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(Asset {
        name: found.name.clone(),
        url: found.browser_download_url.clone(),
        sha256,
        size: found.size,
    })
}

/// What `release` offers a build made from `current_tag` at `current_commit`: nothing when it is the
/// same release, not a published release of the expected form, or has no build to install. The tag
/// names both the launcher and the mod it carries, so a new mod release under the same launcher
/// source is an update too; the commits are shown so the player sees what changed.
pub fn compare(release: &Release, current_tag: &str, current_commit: &str) -> Option<Available> {
    if release.draft || release.prerelease || !valid_tag(&release.tag_name) || !valid_commit(&release.target_commitish) {
        return None;
    }
    if release.tag_name == current_tag {
        return None;
    }
    Some(Available {
        tag: release.tag_name.clone(),
        name: release.name.clone().unwrap_or_else(|| release.tag_name.clone()),
        commit: release.target_commitish.clone(),
        current_tag: current_tag.to_string(),
        current_commit: current_commit.to_string(),
        portable: asset(release, "_portable.exe")?,
        installer: asset(release, "_x64-setup.exe"),
    })
}

/// Asks GitHub whether a newer release is out. Quiet on every failure (offline, rate-limited, a build
/// made outside the release workflow): no answer simply means no update is offered.
pub async fn check() -> Option<Available> {
    let (tag, commit) = (BUILD_TAG?, BUILD_COMMIT?);
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = net::client()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .timeout(CHECK_TIMEOUT)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let release: Release = response.json().await.ok()?;
    compare(&release, tag, commit)
}

/// Downloads `asset` to `to`, reporting (done, total) bytes, and keeps it only when its SHA-256
/// matches; a broken or tampered download is removed.
pub async fn download(asset: &Asset, to: &Path, progress: impl Fn(u64, u64)) -> Result<()> {
    if !asset.url.starts_with(DOWNLOAD_PREFIX) {
        bail!("not a download from {REPO}: {}", asset.url);
    }
    let mut response = net::client()
        .get(&asset.url)
        .send()
        .await
        .with_context(|| format!("GET {}", asset.url))?;
    if !response.status().is_success() {
        bail!("GET {}: HTTP {}", asset.url, response.status());
    }
    let total = response.content_length().unwrap_or(asset.size);
    let partial = sibling(to, ".part");
    let mut file = std::fs::File::create(&partial).with_context(|| format!("cannot write {}", partial.display()))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    progress(0, total);
    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.sync_all()?;
    drop(file);
    let got = hex::encode(hasher.finalize());
    if !got.eq_ignore_ascii_case(&asset.sha256) {
        let _ = std::fs::remove_file(&partial);
        bail!("{} does not match the SHA-256 its release records", asset.name);
    }
    std::fs::rename(&partial, to)?;
    Ok(())
}

/// `path` with `suffix` added to its file name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Where the replaced build is set aside until the next start.
pub fn set_aside(current: &Path) -> PathBuf {
    sibling(current, ".old")
}

/// Puts `new` in the place of the running build `current`. Windows lets a running program be
/// renamed but not overwritten, so the running one is moved aside first; if the new one cannot be
/// moved in, the old one goes back, and nothing is left half done.
pub fn swap(current: &Path, new: &Path) -> Result<()> {
    let old = set_aside(current);
    if old.exists() {
        std::fs::remove_file(&old).with_context(|| format!("cannot clear {}", old.display()))?;
    }
    std::fs::rename(current, &old).with_context(|| format!("cannot move {} aside", current.display()))?;
    if let Err(e) = std::fs::rename(new, current) {
        let _ = std::fs::rename(&old, current);
        return Err(e).with_context(|| format!("cannot put the new build at {}", current.display()));
    }
    Ok(())
}

/// Where a new build is downloaded to, beside the running one so it can be moved in.
pub fn incoming(current: &Path) -> PathBuf {
    sibling(current, ".new")
}

/// Removes what an update left last time: the build set aside, and a download that never finished.
/// The old build may still be closing, so a failure is left for the next start.
pub fn clean_up(current: &Path) {
    let new = incoming(current);
    for leftover in [set_aside(current), sibling(&new, ".part"), new] {
        let _ = std::fs::remove_file(leftover);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "3c03fe0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER: &str = "a457322676f1fb8d370b7c24fb554db1184f7016";
    const DIGEST: &str = "sha256:833eaad8976b1b7b325dd76e777167c5d1025d86f1227c0ac4c169c502740a80";

    fn release(tag: &str, commit: &str) -> Release {
        let file = |name: &str| ReleaseAsset {
            name: name.to_string(),
            browser_download_url: format!("{DOWNLOAD_PREFIX}{tag}/{name}"),
            digest: Some(DIGEST.to_string()),
            size: 1000,
        };
        Release {
            tag_name: tag.to_string(),
            name: Some("霧宿啟動器 0.1.1（霧宿：追放 0.1.0）".to_string()),
            target_commitish: commit.to_string(),
            draft: false,
            prerelease: false,
            assets: vec![
                file("MistinnLauncher_0.1.1_portable.exe"),
                file("MistinnLauncher_0.1.1_x64-setup.exe"),
            ],
        }
    }

    #[test]
    fn tags_have_the_release_form() {
        assert!(valid_tag("v0.1.0-mod.0.1.0"));
        assert!(valid_tag("v12.0.3-mod.1.20.4"));
        for bad in [
            "0.1.0-mod.0.1.0",
            "v0.1.0",
            "v0.1-mod.0.1.0",
            "v0.1.0-mod.0.1.0/../x",
            "v0.1.0-mod.0.1.a",
            "v.1.0-mod.0.1.0",
        ] {
            assert!(!valid_tag(bad), "{bad}");
        }
    }

    #[test]
    fn a_release_from_other_source_is_offered() {
        let offer = compare(&release("v0.1.1-mod.0.1.0", COMMIT), "v0.1.0-mod.0.1.0", OTHER).unwrap();
        assert_eq!(offer.tag, "v0.1.1-mod.0.1.0");
        assert_eq!(offer.commit, COMMIT);
        assert_eq!(offer.portable.name, "MistinnLauncher_0.1.1_portable.exe");
        assert_eq!(offer.portable.sha256, &DIGEST[7..]);
        assert_eq!(offer.installer.unwrap().name, "MistinnLauncher_0.1.1_x64-setup.exe");
    }

    #[test]
    fn the_same_release_is_not_offered() {
        assert_eq!(compare(&release("v0.1.0-mod.0.1.0", OTHER), "v0.1.0-mod.0.1.0", OTHER), None);
    }

    #[test]
    fn a_new_mod_under_the_same_launcher_source_is_offered() {
        let offer = compare(&release("v0.1.0-mod.0.1.1", OTHER), "v0.1.0-mod.0.1.0", OTHER).unwrap();
        assert_eq!(offer.commit, offer.current_commit);
    }

    #[test]
    fn drafts_odd_tags_and_unchecked_files_are_refused() {
        let mut draft = release("v0.1.1-mod.0.1.0", COMMIT);
        draft.draft = true;
        assert_eq!(compare(&draft, "v0.1.0-mod.0.1.0", OTHER), None);

        assert_eq!(compare(&release("nightly", COMMIT), "v0.1.0-mod.0.1.0", OTHER), None);
        assert_eq!(compare(&release("v0.1.1-mod.0.1.0", "main"), "v0.1.0-mod.0.1.0", OTHER), None);

        let mut no_digest = release("v0.1.1-mod.0.1.0", COMMIT);
        no_digest.assets.iter_mut().for_each(|a| a.digest = None);
        assert_eq!(compare(&no_digest, "v0.1.0-mod.0.1.0", OTHER), None);

        let mut elsewhere = release("v0.1.1-mod.0.1.0", COMMIT);
        elsewhere.assets[0].browser_download_url = "https://example.com/MistinnLauncher_0.1.1_portable.exe".to_string();
        assert_eq!(compare(&elsewhere, "v0.1.0-mod.0.1.0", OTHER), None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mistinn-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_swap_puts_the_new_build_in_place_and_sets_the_old_aside() {
        let dir = scratch("swap");
        let current = dir.join("launcher.exe");
        let new = dir.join("launcher.exe.new");
        std::fs::write(&current, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        swap(&current, &new).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"new");
        assert_eq!(std::fs::read(set_aside(&current)).unwrap(), b"old");
        assert!(!new.exists());
        clean_up(&current);
        assert!(!set_aside(&current).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_swap_puts_the_old_build_back() {
        let dir = scratch("rollback");
        let current = dir.join("launcher.exe");
        std::fs::write(&current, b"old").unwrap();
        // the new build is missing, so moving it in fails
        assert!(swap(&current, &dir.join("missing.exe")).is_err());
        assert_eq!(std::fs::read(&current).unwrap(), b"old");
        assert!(!set_aside(&current).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
