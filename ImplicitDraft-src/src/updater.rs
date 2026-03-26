use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::install;

const RELEASES_API: &str = "https://api.github.com/repos/RRCummins/Implicit-draft/releases/latest";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub asset_name: String,
    pub asset_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateResult {
    pub target: PathBuf,
    pub previous_version: String,
    pub installed_version: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub fn latest_available(current_version: &str) -> Result<Option<UpdateInfo>> {
    let release = fetch_latest_release()?;
    let latest_version = normalize_version(&release.tag_name);
    let current_version = normalize_version(current_version);
    if !is_newer_version(&latest_version, &current_version) {
        return Ok(None);
    }

    let asset_name = asset_name_for(&latest_version)?;
    let asset_url = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .map(|asset| asset.browser_download_url.clone())
        .ok_or_else(|| anyhow!("release asset {asset_name} was not found"))?;

    Ok(Some(UpdateInfo {
        current_version,
        latest_version,
        asset_name,
        asset_url,
    }))
}

pub fn self_update(current_version: &str) -> Result<Option<UpdateResult>> {
    let Some(update) = latest_available(current_version)? else {
        return Ok(None);
    };

    let target = install::preferred_install_path();
    let parent = target
        .parent()
        .context("update target is missing a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let temp_path = target.with_extension("download");
    download_file(&update.asset_url, &temp_path)?;
    let install_result = install::install_binary(&temp_path, &target)?;
    fs::remove_file(&temp_path).ok();

    Ok(Some(UpdateResult {
        target: install_result.target,
        previous_version: update.current_version,
        installed_version: update.latest_version,
    }))
}

fn fetch_latest_release() -> Result<ReleaseResponse> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "2",
            "--max-time",
            "5",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: implicit",
            RELEASES_API,
        ])
        .output()
        .context("failed to launch curl for update check")?;

    if !output.status.success() {
        return Err(anyhow!("update check failed"));
    }

    serde_json::from_slice(&output.stdout).context("failed to parse GitHub release metadata")
}

fn download_file(url: &str, destination: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-fsSL", "--connect-timeout", "3", "--max-time", "20", "-o"])
        .arg(destination)
        .arg(url)
        .status()
        .with_context(|| format!("failed to launch curl for {url}"))?;

    if !status.success() {
        return Err(anyhow!("failed to download update from {url}"));
    }

    Ok(())
}

fn asset_name_for(version: &str) -> Result<String> {
    asset_name_for_target(version, std::env::consts::OS, std::env::consts::ARCH)
}

fn asset_name_for_target(version: &str, os: &str, arch: &str) -> Result<String> {
    match (os, arch) {
        ("macos", "aarch64") => Ok(format!("implicit-v{version}-macos-arm64")),
        _ => Err(anyhow!("self-update is unsupported on {os}-{arch}")),
    }
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_owned()
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

fn parse_version(version: &str) -> (u64, u64, u64) {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect::<Vec<_>>();
    parts.resize(3, 0);
    (parts[0], parts[1], parts[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_newer_versions_correctly() {
        assert!(is_newer_version("0.2.0", "0.1.7"));
        assert!(!is_newer_version("0.2.0", "0.2.0"));
        assert!(!is_newer_version("0.1.9", "0.2.0"));
    }

    #[test]
    fn macos_arm_asset_name_matches_release_pattern() {
        let asset = asset_name_for_target("0.2.0", "macos", "aarch64").expect("asset name");
        assert_eq!(asset, "implicit-v0.2.0-macos-arm64");
    }

    #[test]
    fn parses_release_response() {
        let response: ReleaseResponse = serde_json::from_str(
            r#"{
                "tag_name": "v0.2.0",
                "assets": [
                    {
                        "name": "implicit-v0.2.0-macos-arm64",
                        "browser_download_url": "https://example.com/implicit"
                    }
                ]
            }"#,
        )
        .expect("release response");

        assert_eq!(response.tag_name, "v0.2.0");
        assert_eq!(response.assets[0].name, "implicit-v0.2.0-macos-arm64");
    }
}
