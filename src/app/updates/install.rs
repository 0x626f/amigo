//! Native installers and restart helpers. Only verified release assets reach this module.
use amigo_connect::updates::{self, Asset, Format, Target};
use amigo_core::model::id;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use amigo_core::storage;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
#[derive(Clone, Debug)]
pub(super) struct Installation {
    pub target: Target,
    executable: PathBuf,
    destination: PathBuf,
}
pub(super) struct Package {
    pub path: PathBuf,
    pub asset: Asset,
    keep: bool,
}
impl Drop for Package {
    fn drop(&mut self) {
        if !self.keep
            && let Some(parent) = self.path.parent()
        {
            let _ = fs::remove_dir_all(parent);
        }
    }
}
fn successful(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
pub(super) fn detect() -> Result<Installation, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => return Err("Automatic updates are available for x86_64 and ARM64.".into()),
    }
    .to_owned();
    #[cfg(target_os = "linux")]
    let (platform, format, destination) = {
        if let Some(path) = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
        {
            ("linux", Format::AppImage, path)
        } else if executable.starts_with("/usr")
            && successful(
                "rpm",
                &[
                    "-qf",
                    executable.to_str().ok_or("Invalid installation path.")?,
                ],
            )
        {
            ("linux", Format::Rpm, executable.clone())
        } else if executable.starts_with("/usr")
            && successful(
                "dpkg-query",
                &[
                    "-S",
                    executable.to_str().ok_or("Invalid installation path.")?,
                ],
            )
        {
            ("linux", Format::Deb, executable.clone())
        } else {
            let home = storage::user_home(|key| std::env::var_os(key))
                .ok_or("Cannot find your home folder.")?;
            (
                "linux",
                Format::AppImage,
                home.join(".local/bin/Amigo.AppImage"),
            )
        }
    };
    #[cfg(target_os = "macos")]
    let (platform, format, destination) = {
        let bundle = executable
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .map(Path::to_path_buf);
        let destination = bundle
            .or_else(|| {
                storage::user_home(|key| std::env::var_os(key))
                    .map(|p| p.join("Applications/Amigo.app"))
            })
            .ok_or("Cannot locate the Amigo application bundle.")?;
        ("macos", Format::MacZip, destination)
    };
    #[cfg(target_os = "windows")]
    let (platform, format, destination) = (
        "windows",
        Format::WindowsInstaller,
        executable
            .parent()
            .ok_or("Missing application folder.")?
            .to_path_buf(),
    );
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return Err("Automatic updates are not available on this platform.".into());
    Ok(Installation {
        target: Target {
            platform: platform.into(),
            architecture,
            format,
        },
        executable,
        destination,
    })
}
pub(super) fn download(
    asset: Asset,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
) -> Result<Package, String> {
    if Path::new(&asset.name).components().count() != 1 || asset.name.contains(['/', '\\']) {
        return Err("Invalid update package filename.".into());
    }
    let folder = std::env::temp_dir().join(format!("amigo-update-{}", id()));
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let package = Package {
        path: folder.join(&asset.name),
        asset,
        keep: false,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&package.path)
        .map_err(|e| e.to_string())?;
    updates::download(&package.asset, &mut file, &cancel, |bytes| {
        progress.store(bytes, Ordering::Relaxed);
    })?;
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(package)
}
fn launch(command: &mut Command) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0200);
    }

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot start update helper: {e}"))
}
fn native_install(program: &str, package: &Path) -> Result<(), String> {
    let output=Command::new("pkexec").arg(program).args(["-y","install"]).arg(package).output().map_err(|e|format!("Cannot start the system package installer: {e}. Install polkit and its desktop authentication agent."))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "The system installer did not complete ({}). {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(1500)
                .collect::<String>()
        ))
    }
}
pub(super) fn apply(mut package: Package, installation: Installation) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(&package.path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != package.asset.size {
        return Err("The downloaded update changed. Download it again.".into());
    }
    let mut hash = Sha256::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        let read = file.read(&mut chunk).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
    }
    if format!("{:x}", hash.finalize()) != package.asset.sha256 {
        return Err("The downloaded update failed verification. Download it again.".into());
    }
    drop(file);
    let cache = package
        .path
        .parent()
        .ok_or("Missing update folder.")?
        .to_path_buf();
    match package.asset.format {
        Format::Rpm | Format::Deb => {
            native_install(
                if package.asset.format == Format::Rpm {
                    "/usr/bin/dnf"
                } else {
                    "/usr/bin/apt-get"
                },
                &package.path,
            )?;
            let script = cache.join("restart.sh");
            fs::write(&script, include_str!("helpers/restart.sh")).map_err(|e| e.to_string())?;
            launch(
                Command::new("/bin/sh")
                    .arg(&script)
                    .arg(std::process::id().to_string())
                    .arg(&installation.executable)
                    .arg(&cache),
            )?;
            // The shell has opened the script before cleanup; keep it until the helper exits.
            package.keep = true;
        }
        Format::AppImage => {
            let parent = installation
                .destination
                .parent()
                .ok_or("Missing AppImage folder.")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            if fs::symlink_metadata(&installation.destination)
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err("The AppImage update target must not be a symbolic link.".into());
            }
            let staged = parent.join(format!(".amigo-update-{}.AppImage", id()));
            fs::copy(&package.path, &staged).map_err(|e| {
                format!("Cannot stage the AppImage in its installation folder: {e}")
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
            let backup = parent.join(format!(".amigo-previous-{}.AppImage", id()));
            let script = cache.join("appimage.sh");
            fs::write(&script, include_str!("helpers/appimage.sh")).map_err(|e| e.to_string())?;
            if let Err(e) = launch(
                Command::new("/bin/sh")
                    .arg(&script)
                    .arg(std::process::id().to_string())
                    .arg(&installation.destination)
                    .arg(&staged)
                    .arg(backup)
                    .arg(&cache),
            ) {
                let _ = fs::remove_file(staged);
                return Err(e);
            }
            package.keep = true;
        }
        Format::WindowsInstaller => {
            let script = cache.join("install.ps1");
            fs::write(&script, include_str!("helpers/windows.ps1")).map_err(|e| e.to_string())?;
            launch(
                Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                    ])
                    .arg(&script)
                    .arg("-ParentProcess")
                    .arg(std::process::id().to_string())
                    .arg("-Installer")
                    .arg(&package.path)
                    .arg("-Executable")
                    .arg(&installation.executable)
                    .arg("-InstallDirectory")
                    .arg(&installation.destination)
                    .arg("-Cache")
                    .arg(&cache),
            )?;
            package.keep = true;
        }
        Format::MacZip => {
            let expanded = cache.join("expanded");
            if !Command::new("/usr/bin/ditto")
                .args(["-x", "-k"])
                .arg(&package.path)
                .arg(&expanded)
                .status()
                .map_err(|e| e.to_string())?
                .success()
            {
                return Err("The downloaded app bundle could not be extracted.".into());
            }
            let source = expanded.join("Amigo.app");
            if !source.join("Contents/MacOS/amigo").is_file()
                || !successful(
                    "/usr/bin/codesign",
                    &[
                        "--verify",
                        "--deep",
                        "--strict",
                        source.to_str().ok_or("Invalid app bundle path.")?,
                    ],
                )
            {
                return Err("The downloaded app bundle did not pass verification.".into());
            }
            let script = cache.join("macos.sh");
            fs::write(&script, include_str!("helpers/macos.sh")).map_err(|e| e.to_string())?;
            let installer = cache.join("macos-install.sh");
            fs::write(&installer, include_str!("helpers/macos-install.sh"))
                .map_err(|e| e.to_string())?;
            let backup = installation
                .destination
                .with_file_name(format!(".Amigo-previous-{}.app", id()));
            launch(
                Command::new("/bin/sh")
                    .arg(&script)
                    .arg(std::process::id().to_string())
                    .arg(source)
                    .arg(&installation.destination)
                    .arg(backup)
                    .arg(&cache)
                    .arg(installer),
            )?;
            package.keep = true;
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn appimage_helper_replaces_only_the_application_and_quotes_paths() {
        let root = std::env::temp_dir().join(format!("amigo-helper-{} space", id()));
        fs::create_dir(&root).unwrap();
        let target = root.join("Amigo $(not-a-command).AppImage");
        let stage = root.join("new.AppImage");
        let backup = root.join("previous.AppImage");
        let cache = root.join("cache");
        fs::create_dir(&cache).unwrap();
        fs::write(&target, "old app").unwrap();
        let new = b"#!/bin/sh\nprintf 'started' > \"$0.started\"\n";
        fs::write(&stage, new).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o755)).unwrap();
        let script = cache.join("apply.sh");
        fs::write(&script, include_str!("helpers/appimage.sh")).unwrap();
        fs::write(root.join("notes.md"), "User data").unwrap();
        let status = Command::new("/bin/sh")
            .arg(script)
            .arg(i32::MAX.to_string())
            .arg(&target)
            .arg(stage)
            .arg(&backup)
            .arg(&cache)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read(&target).unwrap(), new);
        assert!(!backup.exists());
        assert!(!cache.exists());
        assert_eq!(
            fs::read_to_string(root.join("notes.md")).unwrap(),
            "User data"
        );
        assert!(
            target
                .with_file_name(format!(
                    "{}.started",
                    target.file_name().unwrap().to_string_lossy()
                ))
                .is_file()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
