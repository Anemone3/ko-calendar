pub fn supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux"))
}

pub fn enabled() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        windows::enabled()
    }
    #[cfg(target_os = "linux")]
    {
        linux::enabled(&linux::path()?)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    Ok(false)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::set_enabled(enabled)
    }
    #[cfg(target_os = "linux")]
    {
        linux::set_enabled(
            &linux::path()?,
            enabled,
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = enabled;
        Err("El inicio automático no está disponible en este sistema.".into())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::path::{Path, PathBuf};
    use windows_registry::{CURRENT_USER, Key};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "ko-calendar";
    const NOT_FOUND: u32 = 0x80070002;

    pub(super) fn enabled() -> Result<bool, String> {
        match CURRENT_USER.open(RUN_KEY) {
            Ok(key) => read(&key),
            Err(error) if error.code().0 as u32 == NOT_FOUND => Ok(false),
            Err(error) => Err(format!("No se pudo leer el inicio automático: {error}")),
        }
    }

    fn read(key: &Key) -> Result<bool, String> {
        match key.get_string(VALUE) {
            Ok(command) => Ok(!command.is_empty()),
            Err(error) if error.code().0 as u32 == NOT_FOUND => Ok(false),
            Err(error) => Err(format!("No se pudo leer el inicio automático: {error}")),
        }
    }

    pub(super) fn set_enabled(enabled: bool) -> Result<(), String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let key = CURRENT_USER.create(RUN_KEY).map_err(|e| e.to_string())?;
        write(&key, enabled, &launch_path(&executable))
    }

    // Scoop keeps its current junction stable when replacing version directories.
    fn launch_path(executable: &Path) -> PathBuf {
        if let Some(version) = executable.parent()
            && let Some(name) = version.file_name().and_then(|name| name.to_str())
            && name.split('.').count() == 3
            && name
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
            && let (Some(root), Some(file)) = (version.parent(), executable.file_name())
        {
            let current = root.join("current").join(file);
            if current.is_file() {
                return current;
            }
        }
        executable.into()
    }

    fn write(key: &Key, enabled: bool, executable: &Path) -> Result<(), String> {
        if enabled {
            let executable = executable
                .to_str()
                .ok_or("La ruta de la app no es válida.")?;
            let command = format!("\"{executable}\" --tray");
            if command.encode_utf16().count() > 260 {
                return Err(
                    "La ruta de la app es demasiado larga para el inicio automático de Windows."
                        .into(),
                );
            }
            key.set_string(VALUE, command)
                .map_err(|e| format!("No se pudo activar el inicio automático: {e}"))
        } else {
            match key.remove_value(VALUE) {
                Ok(()) => Ok(()),
                Err(error) if error.code().0 as u32 == NOT_FOUND => Ok(()),
                Err(error) => Err(format!(
                    "No se pudo desactivar el inicio automático: {error}"
                )),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn registry_toggle_roundtrips_without_touching_the_real_startup_key() {
            let path = format!(
                r"Software\ko-calendar\Tests\Startup-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            );
            let key = CURRENT_USER.create(&path).unwrap();
            let executable = Path::new(r"C:\Apps with spaces\ko-calendar.exe");
            assert!(!read(&key).unwrap());
            write(&key, true, executable).unwrap();
            assert!(read(&key).unwrap());
            assert_eq!(
                key.get_string(VALUE).unwrap(),
                r#""C:\Apps with spaces\ko-calendar.exe" --tray"#
            );
            write(&key, false, executable).unwrap();
            write(&key, false, executable).unwrap();
            assert!(!read(&key).unwrap());
            drop(key);
            CURRENT_USER.remove_tree(path).unwrap();
        }

        #[test]
        fn scoop_startup_uses_the_current_junction_across_upgrades() {
            let data = super::super::tests::TestDirectory::new();
            let current = data.0.join("current").join("ko-calendar.exe");
            std::fs::create_dir_all(current.parent().unwrap()).unwrap();
            std::fs::write(&current, b"app").unwrap();
            assert_eq!(
                launch_path(&data.0.join("0.4.7").join("ko-calendar.exe")),
                current
            );
            let portable = data.0.join("portable").join("ko-calendar.exe");
            assert_eq!(launch_path(&portable), portable);
        }
    }
}

#[cfg(any(target_os = "linux", test))]
mod linux {
    use std::{fs, path::Path};

    #[cfg(target_os = "linux")]
    pub(super) fn path() -> Result<std::path::PathBuf, String> {
        directories::BaseDirs::new()
            .map(|base| base.config_dir().join("autostart/ko-calendar.desktop"))
            .ok_or_else(|| "No se encontró la carpeta de configuración del usuario.".into())
    }

    pub(super) fn enabled(path: &Path) -> Result<bool, String> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(format!("No se pudo leer el inicio automático: {error}")),
        };
        let mut in_entry = false;
        let mut found = false;
        for line in contents.lines().map(str::trim) {
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
                found |= in_entry;
            } else if in_entry
                && let Some((key, value)) = line.split_once('=')
                && ((key.trim() == "Hidden" && value.trim() == "true")
                    || (key.trim() == "X-GNOME-Autostart-enabled" && value.trim() == "false"))
            {
                return Ok(false);
            }
        }
        Ok(found)
    }

    pub(super) fn set_enabled(path: &Path, enabled: bool, executable: &Path) -> Result<(), String> {
        let contents = if enabled {
            let executable = executable
                .to_str()
                .ok_or("La ruta de la app no es válida.")?;
            if executable.contains('=') || executable.chars().any(char::is_control) {
                return Err(
                    "La ruta de la app no es compatible con el inicio automático del escritorio."
                        .into(),
                );
            }
            // Desktop Entry applies string escaping before Exec argument unquoting.
            let quoted = executable
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
                .replace('%', "%%");
            let escaped = quoted.replace('\\', "\\\\");
            format!(
                "[Desktop Entry]\nType=Application\nName=ko-calendar\nExec=\"{escaped}\" --tray\nTerminal=false\nStartupNotify=false\nHidden=false\n"
            )
        } else {
            "[Desktop Entry]\nType=Application\nName=ko-calendar\nHidden=true\n".into()
        };
        let parent = path
            .parent()
            .ok_or("La carpeta de inicio automático no es válida.")?;
        fs::create_dir_all(parent)
            .and_then(|_| fs::write(path, contents))
            .map_err(|e| format!("No se pudo guardar el inicio automático: {e}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn xdg_autostart_toggle_roundtrips_and_respects_disabled_entries() {
            let data = super::super::tests::TestDirectory::new();
            let path = data.0.join("autostart/ko-calendar.desktop");
            assert!(!enabled(&path).unwrap());
            set_enabled(&path, true, Path::new("/opt/My Apps/ko-calendar")).unwrap();
            assert!(enabled(&path).unwrap());
            assert!(
                fs::read_to_string(&path)
                    .unwrap()
                    .contains("Exec=\"/opt/My Apps/ko-calendar\" --tray\n")
            );
            set_enabled(&path, false, Path::new("/opt/My Apps/ko-calendar")).unwrap();
            assert!(!enabled(&path).unwrap());
            fs::write(&path, "[Desktop Entry]\nX-GNOME-Autostart-enabled=false\n").unwrap();
            assert!(!enabled(&path).unwrap());
            set_enabled(&path, true, Path::new("/opt/ko-calendar")).unwrap();
            assert!(enabled(&path).unwrap());
        }

        #[test]
        fn desktop_exec_escapes_reserved_characters_and_rejects_newlines() {
            let data = super::super::tests::TestDirectory::new();
            let path = data.0.join("autostart/ko-calendar.desktop");
            set_enabled(&path, true, Path::new("/opt/$Apps/100%/ko-calendar")).unwrap();
            assert!(
                fs::read_to_string(&path)
                    .unwrap()
                    .contains(r#"Exec="/opt/\\$Apps/100%%/ko-calendar" --tray"#)
            );
            assert!(
                set_enabled(&path, true, Path::new("/opt/bad\nExec=other/ko-calendar")).is_err()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    pub(super) struct TestDirectory(pub PathBuf);
    impl TestDirectory {
        pub fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "ko-calendar-startup-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            )))
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
