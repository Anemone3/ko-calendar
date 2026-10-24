use kokonut_calendar::sound_settings::{ADVANCE_FILE, DEFAULT_FILE, RENAMED_FILES, current_file};
use rodio::{Decoder, OutputStreamBuilder, Sink, Source};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

const DEFAULT_SOUND: &[u8] = include_bytes!("../assets/sounds/ko-tify.mp3");
const ADVANCE_SOUND: &[u8] = include_bytes!("../assets/sounds/alarm.mp3");

pub fn directory(data: &Path) -> PathBuf {
    data.join("sounds")
}

pub fn initialize(data: &Path) -> Result<Vec<String>, String> {
    let directory = directory(data);
    fs::create_dir_all(&directory)
        .map_err(|e| format!("No se pudo crear la carpeta de sonidos: {e}"))?;
    for (previous, current) in RENAMED_FILES {
        let previous = directory.join(previous);
        let current = directory.join(current);
        if previous.is_file() && !current.exists() {
            fs::rename(previous, current)
                .map_err(|e| format!("No se pudo actualizar el nombre del sonido: {e}"))?;
        }
    }
    for (name, embedded) in [(DEFAULT_FILE, DEFAULT_SOUND), (ADVANCE_FILE, ADVANCE_SOUND)] {
        let default = directory.join(name);
        if default.exists() {
            continue;
        }
        let installed = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("sounds").join(name)));
        let bytes = installed
            .and_then(|p| fs::read(p).ok())
            .unwrap_or_else(|| embedded.to_vec());
        fs::write(&default, bytes).map_err(|e| format!("No se pudo instalar el sonido: {e}"))?;
    }
    list_files(data)
}

/// Re-scan the same directory opened by the settings panel, without reinstalling defaults.
pub fn list_files(data: &Path) -> Result<Vec<String>, String> {
    let mut files = fs::read_dir(directory(data))
        .map_err(|e| format!("No se pudo leer la carpeta de sonidos: {e}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_name(name) && current_file(name) == name)
        .collect::<Vec<_>>();
    files.sort_by_key(|name| name.to_lowercase());
    Ok(files)
}

fn valid_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !name.contains(['/', '\\', ':'])
        && Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "mp3" | "wav" | "ogg" | "oga" | "flac" | "m4a" | "aac"
                )
            })
}

pub fn file_path(data: &Path, name: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err(
            "Elige un audio MP3, WAV, OGG, FLAC, M4A o AAC de la carpeta de sonidos.".into(),
        );
    }
    let path = directory(data).join(name);
    let metadata = fs::metadata(&path)
        .map_err(|_| "No se encontró el sonido. Revisa la carpeta de sonidos.".to_string())?;
    if !metadata.is_file() {
        return Err("Elige un archivo de audio de la carpeta de sonidos.".into());
    }
    let size = metadata.len();
    if size == 0 || size > 20 * 1024 * 1024 {
        return Err("Usa un archivo de audio de entre 1 byte y 20 MB.".into());
    }
    Ok(path)
}

#[derive(Clone, Default)]
pub struct Player {
    generation: Arc<AtomicU64>,
}

impl Player {
    pub fn next(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1)
    }
    pub fn stop(&self) {
        self.next();
    }

    pub fn play(&self, path: PathBuf, volume: u8, generation: u64) -> Result<(), String> {
        let file = fs::File::open(path).map_err(|e| format!("No se pudo abrir el sonido: {e}"))?;
        let source = Decoder::try_from(file).map_err(|_| {
            "El archivo no contiene audio válido en un formato compatible.".to_string()
        })?;
        if self.generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        let mut stream = OutputStreamBuilder::open_default_stream().map_err(|_| {
            "No se pudo abrir la salida de audio. Comprueba tus altavoces o auriculares."
                .to_string()
        })?;
        stream.log_on_drop(false);
        let sink = Sink::connect_new(stream.mixer());
        sink.set_volume(f32::from(volume.min(100)) / 100.);
        // An alert is one short playback, never an unbounded loop or a long music track.
        sink.append(source.take_duration(Duration::from_secs(30)));
        while !sink.empty() {
            if self.generation.load(Ordering::SeqCst) != generation {
                sink.stop();
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_data() -> PathBuf {
        let data = std::env::temp_dir().join(format!(
            "ko-calendar-sounds-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(directory(&data)).unwrap();
        data
    }
    #[test]
    fn legacy_sound_files_keep_their_contents_under_the_new_names() {
        let data = test_data();
        fs::write(directory(&data).join("kokonut.mp3"), b"my notification").unwrap();
        fs::write(directory(&data).join("anticipado.mp3"), b"my alarm").unwrap();
        fs::write(directory(&data).join("personal.mp3"), b"my custom sound").unwrap();
        let files = initialize(&data).unwrap();
        assert_eq!(files, ["alarm.mp3", "ko-tify.mp3", "personal.mp3"]);
        assert_eq!(
            fs::read(directory(&data).join("ko-tify.mp3")).unwrap(),
            b"my notification"
        );
        assert_eq!(
            fs::read(directory(&data).join("alarm.mp3")).unwrap(),
            b"my alarm"
        );
        assert_eq!(
            fs::read(directory(&data).join("personal.mp3")).unwrap(),
            b"my custom sound"
        );
        assert!(!directory(&data).join("kokonut.mp3").exists());
        assert!(!directory(&data).join("anticipado.mp3").exists());
        assert_eq!(initialize(&data).unwrap(), files);
        fs::remove_dir_all(data).unwrap();
    }
    #[test]
    fn existing_new_sound_files_are_never_overwritten_by_legacy_files() {
        let data = test_data();
        fs::write(directory(&data).join("kokonut.mp3"), b"previous sound").unwrap();
        fs::write(directory(&data).join("ko-tify.mp3"), b"new sound").unwrap();
        let files = initialize(&data).unwrap();
        assert_eq!(files, ["alarm.mp3", "ko-tify.mp3"]);
        assert_eq!(
            fs::read(directory(&data).join("ko-tify.mp3")).unwrap(),
            b"new sound"
        );
        assert_eq!(
            fs::read(directory(&data).join("kokonut.mp3")).unwrap(),
            b"previous sound"
        );
        fs::remove_dir_all(data).unwrap();
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(
                std::env::temp_dir()
                    .join(format!("kokonut-sounds-{}-{unique}", std::process::id())),
            )
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn added_audio_files_appear_on_refresh_and_restart_and_removals_disappear() {
        let data = TestDirectory::new();
        assert_eq!(initialize(&data.0).unwrap().len(), 2);
        let folder = directory(&data.0);
        let names = [
            "Mi aviso.MP3",
            "audio.wav",
            "audio.OGG",
            "audio.oga",
            "audio.flac",
            "audio.m4a",
            "audio.aac",
        ];
        for name in names {
            fs::write(folder.join(name), DEFAULT_SOUND).unwrap();
        }
        fs::write(folder.join("notas.txt"), "not audio").unwrap();
        fs::create_dir(folder.join("carpeta.mp3")).unwrap();

        let files = list_files(&data.0).unwrap();
        assert_eq!(files.len(), names.len() + 2);
        for name in names {
            assert!(files.contains(&name.to_string()), "{name}");
            assert_eq!(file_path(&data.0, name).unwrap(), folder.join(name));
        }
        assert!(
            files
                .windows(2)
                .all(|pair| pair[0].to_lowercase() <= pair[1].to_lowercase())
        );
        assert!(file_path(&data.0, "carpeta.mp3").is_err());
        assert_eq!(initialize(&data.0).unwrap(), files);
        assert_eq!(
            fs::read(folder.join("Mi aviso.MP3")).unwrap(),
            DEFAULT_SOUND
        );

        fs::remove_file(folder.join("Mi aviso.MP3")).unwrap();
        assert!(
            !list_files(&data.0)
                .unwrap()
                .contains(&"Mi aviso.MP3".to_string())
        );
        assert!(file_path(&data.0, "Mi aviso.MP3").is_err());
    }

    #[test]
    fn wav_audio_can_be_decoded() {
        // Eight PCM samples, mono, 8 kHz, 16 bit.
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&52_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8000_u32.to_le_bytes());
        wav.extend_from_slice(&16000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        for _ in 0..8 {
            wav.extend_from_slice(&1000_i16.to_le_bytes());
        }
        let decoder = Decoder::try_from(std::io::Cursor::new(wav)).unwrap();
        assert_eq!(decoder.sample_rate(), 8000);
        assert_eq!(decoder.channels(), 1);
        assert_eq!(decoder.count(), 8);
    }

    #[test]
    fn supplied_sound_decodes_to_a_short_non_silent_clip() {
        let decoder = Decoder::try_from(std::io::Cursor::new(DEFAULT_SOUND)).unwrap();
        let rate = decoder.sample_rate();
        let channels = decoder.channels();
        let samples: Vec<f32> = decoder.collect();
        let duration = samples.len() as f32 / rate as f32 / channels as f32;
        assert!((0.7..1.0).contains(&duration));
        assert!(samples.iter().all(|sample| sample.is_finite()));
        assert!(samples.iter().any(|sample| sample.abs() > 0.05));
    }
    #[test]
    fn sound_choices_cannot_escape_the_sound_directory() {
        for name in [
            "../alarm.mp3",
            "..\\alarm.mp3",
            "C:\\alarm.mp3",
            "alarm.mp3:stream",
            "/alarm.mp3",
            "https://example.com/a.mp3",
            "alarm.txt",
        ] {
            assert!(!valid_name(name), "{name}");
        }
        assert!(valid_name("Mi aviso.MP3"));
        for name in [
            "aviso.wav",
            "aviso.OGG",
            "aviso.oga",
            "aviso.flac",
            "aviso.m4a",
            "aviso.aac",
        ] {
            assert!(valid_name(name), "{name}");
        }
    }
    #[test]
    fn advance_audio_is_distinct_non_silent_and_fits_the_playback_limit() {
        assert_ne!(DEFAULT_SOUND, ADVANCE_SOUND);
        let decoder = Decoder::try_from(std::io::Cursor::new(ADVANCE_SOUND)).unwrap();
        let rate = decoder.sample_rate();
        let channels = decoder.channels();
        let samples: Vec<f32> = decoder.collect();
        let duration = samples.len() as f32 / rate as f32 / channels as f32;
        assert!((0.1..=30.0).contains(&duration));
        assert!(samples.iter().all(|v| v.is_finite()));
        assert!(samples.iter().any(|v| v.abs() > 0.05));
    }
}
