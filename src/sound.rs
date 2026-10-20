use rodio::{Decoder, OutputStreamBuilder, Sink, Source};
use std::{
    fs,
    io::BufReader,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub const DEFAULT_FILE: &str = "kokonut.mp3";
const DEFAULT_SOUND: &[u8] = include_bytes!("../assets/sounds/ko-tify.mp3");
pub const ADVANCE_FILE: &str = "anticipado.mp3";
const ADVANCE_SOUND: &[u8] = include_bytes!("../assets/sounds/alarm.mp3");

pub fn directory(data: &Path) -> PathBuf {
    data.join("sounds")
}

pub fn initialize(data: &Path) -> Result<Vec<String>, String> {
    let directory = directory(data);
    fs::create_dir_all(&directory)
        .map_err(|e| format!("No se pudo crear la carpeta de sonidos: {e}"))?;
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
    let mut files = fs::read_dir(&directory)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_name(name))
        .collect::<Vec<_>>();
    files.sort_by_key(|name| name.to_lowercase());
    Ok(files)
}

fn valid_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !name.contains(['/', '\\', ':'])
        && name.to_ascii_lowercase().ends_with(".mp3")
}

pub fn file_path(data: &Path, name: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err("Elige un archivo MP3 de la carpeta de sonidos.".into());
    }
    let path = directory(data).join(name);
    let size = fs::metadata(&path)
        .map_err(|_| "No se encontró el sonido. Revisa la carpeta de sonidos.".to_string())?
        .len();
    if size == 0 || size > 20 * 1024 * 1024 {
        return Err("Usa un MP3 de entre 1 byte y 20 MB.".into());
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
        let source = Decoder::try_from(BufReader::new(file))
            .map_err(|_| "El archivo no contiene un MP3 válido.".to_string())?;
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
            "alarm.wav",
        ] {
            assert!(!valid_name(name), "{name}");
        }
        assert!(valid_name("Mi aviso.MP3"));
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
