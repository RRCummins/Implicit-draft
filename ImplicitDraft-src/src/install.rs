use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallStatus {
    pub target: PathBuf,
    pub installed: bool,
    pub on_path: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallResult {
    pub target: PathBuf,
    pub already_current: bool,
    pub on_path: bool,
}

pub fn current_status() -> InstallStatus {
    let target = preferred_install_path();
    let installed = env::current_exe().ok().as_ref() == Some(&target) || target.exists();
    let on_path = is_dir_on_path(target.parent().unwrap_or_else(|| Path::new(".")));

    InstallStatus {
        target,
        installed,
        on_path,
    }
}

pub fn preferred_install_path() -> PathBuf {
    match env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".local/bin/implicit"),
        None => PathBuf::from(".implicit/bin/implicit"),
    }
}

pub fn path_export_hint() -> &'static str {
    "export PATH=\"$HOME/.local/bin:$PATH\""
}

pub fn install_current_exe() -> Result<InstallResult> {
    let current = env::current_exe().context("failed to locate current executable")?;
    install_binary(&current, &preferred_install_path())
}

pub fn install_binary(source: &Path, target: &Path) -> Result<InstallResult> {
    if source == target {
        return Ok(InstallResult {
            target: target.to_path_buf(),
            already_current: true,
            on_path: is_dir_on_path(target.parent().unwrap_or_else(|| Path::new("."))),
        });
    }

    let parent = target
        .parent()
        .context("install target is missing a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let temp_path = target.with_extension("tmp");
    fs::copy(source, &temp_path).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            temp_path.display()
        )
    })?;

    #[cfg(unix)]
    fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to set permissions on {}", temp_path.display()))?;

    fs::rename(&temp_path, target).with_context(|| {
        format!(
            "failed to move {} to {}",
            temp_path.display(),
            target.display()
        )
    })?;

    Ok(InstallResult {
        target: target.to_path_buf(),
        already_current: false,
        on_path: is_dir_on_path(parent),
    })
}

fn is_dir_on_path(dir: &Path) -> bool {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).any(|entry| entry == dir))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_nanos();
        env::temp_dir().join(format!("implicit-install-{name}-{unique}"))
    }

    #[test]
    fn install_binary_copies_source_to_target() {
        let root = temp_dir("copy");
        fs::create_dir_all(&root).expect("mkdir");
        let source = root.join("source-bin");
        let target = root.join("bin/implicit");
        fs::write(&source, "binary").expect("seed");

        let result = install_binary(&source, &target).expect("install");

        assert_eq!(result.target, target);
        assert_eq!(fs::read_to_string(result.target).expect("target"), "binary");

        fs::remove_dir_all(root).expect("cleanup");
    }
}
