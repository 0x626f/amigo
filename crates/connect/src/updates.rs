//! Stable GitHub releases and streamed, SHA-256-verified native packages.
use reqwest::blocking::Client;
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
pub const REPOSITORY: &str = "https://github.com/0x626f/amigo";
const API: &str = "https://api.github.com/repos/0x626f/amigo/releases/latest";
const MAX_PACKAGE: u64 = 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Rpm,
    Deb,
    AppImage,
    MacZip,
    WindowsInstaller,
}
impl Format {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Rpm => "rpm",
            Self::Deb => "deb",
            Self::AppImage => "AppImage",
            Self::MacZip => "app.zip",
            Self::WindowsInstaller => "setup.exe",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Target {
    pub platform: String,
    pub architecture: String,
    pub format: Format,
}
#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub format: Format,
}
#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub notes: String,
    pub package: Option<Asset>,
}
fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(format!("Amigo/{}", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = attempt.url().host_str().unwrap_or("");
            if attempt.previous().len() < 5
                && attempt.url().scheme() == "https"
                && [
                    "github.com",
                    "release-assets.githubusercontent.com",
                    "objects.githubusercontent.com",
                ]
                .contains(&host)
            {
                attempt.follow()
            } else {
                attempt.error("Unexpected update download redirect")
            }
        }))
        .build()
        .map_err(|e| format!("Cannot initialize update client: {e}"))
}
pub fn latest(current: &str, target: &Target) -> Result<Option<Release>, String> {
    let response = client()?
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .header("Cache-Control", "no-cache")
        .send()
        .map_err(|e| format!("Cannot check GitHub for updates: {e}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err("GitHub's request limit was reached. Try again later.".into());
    }
    let response = response
        .error_for_status()
        .map_err(|e| format!("Update check failed: {e}"))?;
    let mut bytes = vec![];
    response
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("GitHub returned an oversized release response.".into());
    }
    parse_release(
        &serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
        current,
        target,
    )
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn parse_release(
    value: &Value,
    current: &str,
    target: &Target,
) -> Result<Option<Release>, String> {
    if value["draft"] == true || value["prerelease"] == true {
        return Ok(None);
    }
    let tag = value["tag_name"]
        .as_str()
        .ok_or("The release has no version tag.")?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .map_err(|_| "The release has an invalid version.")?;
    let installed = Version::parse(current.strip_prefix('v').unwrap_or(current))
        .map_err(|_| "The installed version is invalid.")?;
    if !version.pre.is_empty() || version <= installed {
        return Ok(None);
    }
    let expected_url = format!("{REPOSITORY}/releases/tag/{tag}");
    if value["html_url"].as_str() != Some(expected_url.as_str()) {
        return Err("The update release URL does not match the Amigo repository.".into());
    }
    let suffix = format!(
        "-{}-{}.{}",
        target.platform,
        target.architecture,
        target.format.suffix()
    );
    let prefix = format!("amigo-{version}");
    let mut candidates = vec![];
    for asset in value["assets"].as_array().into_iter().flatten() {
        let Some(name) = asset["name"].as_str() else {
            continue;
        };
        if asset["state"] != "uploaded" || !name.ends_with(&suffix) {
            continue;
        }
        let Some(revision) = name
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(&suffix))
        else {
            continue;
        };
        let revision = if revision.is_empty() {
            0
        } else {
            match revision
                .strip_prefix('-')
                .and_then(|s| s.parse::<u32>().ok())
            {
                Some(n) => n,
                None => continue,
            }
        };
        let url = format!("{REPOSITORY}/releases/download/{tag}/{name}");
        if asset["browser_download_url"].as_str() != Some(url.as_str()) {
            return Err("The update package URL does not match the Amigo release.".into());
        }
        let size = asset["size"]
            .as_u64()
            .ok_or("The update package has no size.")?;
        if size == 0 || size > MAX_PACKAGE {
            return Err("The update package size is invalid.".into());
        }
        let hash = asset["digest"]
            .as_str()
            .and_then(|s| s.strip_prefix("sha256:"))
            .filter(|s| valid_hash(s))
            .ok_or(
                "The release package is missing its SHA-256 digest. Installation is disabled.",
            )?;
        candidates.push((
            revision,
            Asset {
                name: name.into(),
                url,
                size,
                sha256: hash.to_ascii_lowercase(),
                format: target.format,
            },
        ));
    }
    candidates.sort_by_key(|(revision, _)| *revision);
    Ok(Some(Release {
        version: version.to_string(),
        url: expected_url,
        notes: value["body"].as_str().unwrap_or("").into(),
        package: candidates.pop().map(|(_, asset)| asset),
    }))
}
pub fn download(
    asset: &Asset,
    writer: &mut impl Write,
    cancel: &AtomicBool,
    progress: impl FnMut(u64),
) -> Result<(), String> {
    if !asset
        .url
        .starts_with(&format!("{REPOSITORY}/releases/download/"))
        || !valid_hash(&asset.sha256)
        || asset.size == 0
        || asset.size > MAX_PACKAGE
    {
        return Err("Invalid update package metadata.".into());
    }
    let response = client()?
        .get(&asset.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Cannot download update: {e}"))?;
    copy_verified(
        response,
        writer,
        asset.size,
        &asset.sha256,
        cancel,
        progress,
    )
}
fn copy_verified(
    mut reader: impl Read,
    writer: &mut impl Write,
    expected_size: u64,
    expected_hash: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    let mut total = 0;
    let mut hash = Sha256::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Update download cancelled.".into());
        }
        let read = reader
            .read(&mut chunk)
            .map_err(|e| format!("Update download interrupted: {e}"))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > expected_size || total > MAX_PACKAGE {
            return Err("The update download exceeds its declared size.".into());
        }
        writer
            .write_all(&chunk[..read])
            .map_err(|e| format!("Cannot store update package: {e}"))?;
        hash.update(&chunk[..read]);
        progress(total);
    }
    if total != expected_size
        || format!("{:x}", hash.finalize()) != expected_hash.to_ascii_lowercase()
    {
        return Err(
            "The update package failed SHA-256 verification. Nothing was installed.".into(),
        );
    }
    writer.flush().map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn target() -> Target {
        Target {
            platform: "linux".into(),
            architecture: "x86_64".into(),
            format: Format::Rpm,
        }
    }
    fn release() -> Value {
        serde_json::json!({"tag_name":"v0.0.2","html_url":format!("{REPOSITORY}/releases/tag/v0.0.2"),"draft":false,"prerelease":false,"assets":[{"name":"amigo-0.0.2-linux-x86_64.rpm","browser_download_url":format!("{REPOSITORY}/releases/download/v0.0.2/amigo-0.0.2-linux-x86_64.rpm"),"size":3,"digest":format!("sha256:{:x}",Sha256::digest(b"rpm")),"state":"uploaded"}]})
    }
    #[test]
    fn versions_are_semantic_and_stable_releases_only() {
        let r = release();
        assert!(
            parse_release(&r, "0.0.1", &target())
                .unwrap()
                .unwrap()
                .package
                .is_some()
        );
        assert!(parse_release(&r, "0.0.2", &target()).unwrap().is_none());
        assert!(parse_release(&r, "0.0.10", &target()).unwrap().is_none());
        let mut r = r;
        r["prerelease"] = true.into();
        assert!(parse_release(&r, "0.0.1", &target()).unwrap().is_none());
    }
    #[test]
    fn mismatched_architectures_never_install() {
        let mut t = target();
        t.architecture = "aarch64".into();
        assert!(
            parse_release(&release(), "0.0.1", &t)
                .unwrap()
                .unwrap()
                .package
                .is_none()
        );
    }
    #[test]
    fn bad_urls_digests_and_sizes_are_rejected() {
        for (key, value) in [
            (
                "browser_download_url",
                Value::from("https://evil.example/package.rpm"),
            ),
            ("digest", Value::Null),
            ("size", Value::from(0)),
        ] {
            let mut r = release();
            r["assets"][0][key] = value;
            assert!(parse_release(&r, "0.0.1", &target()).is_err(), "{key}");
        }
    }
    #[test]
    fn checksums_truncation_oversize_and_cancel_prevent_installation() {
        let digest = format!("{:x}", Sha256::digest(b"rpm"));
        let mut out = vec![];
        let cancel = AtomicBool::new(false);
        copy_verified(&b"rpm"[..], &mut out, 3, &digest, &cancel, |_| {}).unwrap();
        assert_eq!(out, b"rpm");
        for bytes in [&b"bad"[..], &b"rp"[..], &b"rpmm"[..]] {
            assert!(copy_verified(bytes, &mut vec![], 3, &digest, &cancel, |_| {}).is_err());
        }
        cancel.store(true, Ordering::Relaxed);
        assert!(copy_verified(&b"rpm"[..], &mut vec![], 3, &digest, &cancel, |_| {}).is_err());
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    #[test]
    #[ignore = "requires public GitHub API access"]
    fn live_amigo_release_metadata() {
        let target = Target {
            platform: "linux".into(),
            architecture: "x86_64".into(),
            format: Format::Rpm,
        };
        let release = latest("0.0.0", &target).unwrap().unwrap();
        assert!(release.package.is_some());
        assert!(release.url.starts_with(REPOSITORY));
    }
}
