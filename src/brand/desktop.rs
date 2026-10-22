//! Wayland shells resolve icons through the desktop entry matching the app ID.
use std::{env, fs, io, path::Path};

pub fn register_desktop_icon() -> io::Result<()> {
    let user_data = env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .ok_or_else(|| io::Error::other("No user application directory is available."))?;
    let system_data =
        env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let installed = env::split_paths(&system_data)
        .chain(std::iter::once(user_data.clone()))
        .any(|root| root.join("applications/amigo.desktop").is_file());
    if installed {
        return Ok(());
    }
    // AppImage executables live in a temporary mount; launch the original image.
    let executable = env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(Ok)
        .unwrap_or_else(env::current_exe)?;
    install(&user_data, &executable)
}

fn install(data: &Path, executable: &Path) -> io::Result<()> {
    let launcher = data.join("applications/amigo.desktop");
    if launcher.exists() {
        return Ok(());
    }
    let command = desktop_argument(executable)?;
    let contents = include_str!("../../packaging/amigo.desktop")
        .replace("Exec=amigo\n", &format!("Exec={command}\n"));
    for (relative, bytes) in [
        (
            "icons/hicolor/scalable/apps/amigo.svg",
            include_bytes!("../../packaging/amigo.svg").as_slice(),
        ),
        (
            "icons/hicolor/256x256/apps/amigo.png",
            include_bytes!("../../packaging/amigo.png").as_slice(),
        ),
        ("applications/amigo.desktop", contents.as_bytes()),
    ] {
        let destination = data.join(relative);
        fs::create_dir_all(destination.parent().unwrap())?;
        fs::write(destination, bytes)?;
    }
    Ok(())
}

fn desktop_argument(path: &Path) -> io::Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| io::Error::other("Application path is not UTF-8."))?;
    let mut quoted = String::from("\"");
    for ch in value.chars() {
        match ch {
            '%' => quoted.push_str("%%"),
            '\\' => quoted.push_str(r"\\\\"),
            '"' => quoted.push_str(r#"\\\""#),
            '$' => quoted.push_str(r"\\$"),
            '`' => quoted.push_str(r"\\`"),
            '\n' | '\r' => return Err(io::Error::other("Application path contains a newline.")),
            ch => quoted.push(ch),
        }
    }
    quoted.push('"');
    Ok(quoted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_launcher_registers_the_logo_and_preserves_existing_entries() {
        let data = env::temp_dir().join(format!(
            "amigo-desktop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        install(&data, Path::new("/tmp/Amigo workspace/amigo")).unwrap();
        let launcher = data.join("applications/amigo.desktop");
        let contents = fs::read_to_string(&launcher).unwrap();
        assert!(contents.contains("Exec=\"/tmp/Amigo workspace/amigo\"\n"));
        assert!(contents.contains("Icon=amigo\n"));
        assert!(contents.contains("StartupWMClass=amigo\n"));
        assert_eq!(
            fs::read(data.join("icons/hicolor/256x256/apps/amigo.png")).unwrap(),
            include_bytes!("../../packaging/amigo.png")
        );
        assert_eq!(
            fs::read(data.join("icons/hicolor/scalable/apps/amigo.svg")).unwrap(),
            include_bytes!("../../packaging/amigo.svg")
        );
        fs::write(&launcher, "User's existing launcher").unwrap();
        install(&data, Path::new("/another/amigo")).unwrap();
        assert_eq!(
            fs::read_to_string(&launcher).unwrap(),
            "User's existing launcher"
        );
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn launcher_paths_escape_desktop_field_codes_and_reserved_characters() {
        assert_eq!(
            desktop_argument(Path::new("/tmp/amigo 100%/$release")).unwrap(),
            r#""/tmp/amigo 100%%/\\$release""#
        );
        assert_eq!(
            desktop_argument(Path::new("/tmp/a\\b\"c`d")).unwrap(),
            r#""/tmp/a\\\\b\\\"c\\`d""#
        );
        assert!(desktop_argument(Path::new("/tmp/amigo\nExec=wrong")).is_err());
    }
}
