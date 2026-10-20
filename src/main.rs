#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod icons;
mod platform;
mod sound;
mod ui;
#[cfg(target_os = "windows")]
mod windows_notifications;

use app::{App, DATA_DIR};
use directories::ProjectDirs;
use fs2::FileExt;
use kokonut_calendar::store::Store;
use std::{cell::RefCell, fs::OpenOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::var_os("KOKONUT_DATA_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            ProjectDirs::from("dev", "kokonut", "Kokonut Calendar")
                .map(|dirs| dirs.data_local_dir().to_owned())
        })
        .ok_or("No se encontró un directorio de datos de usuario")?;
    std::fs::create_dir_all(&directory)?;
    let _ = DATA_DIR.set(directory.clone());
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("kokonut.log"))?;
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ko_calendar=info,kokonut_calendar=info,warn".into()),
        )
        .with_writer(log)
        .try_init();
    let instance = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("instance.lock"))?;
    if instance.try_lock_exclusive().is_err() {
        tracing::info!("Kokonut ya está ejecutándose. Usa su icono de bandeja.");
        return Ok(());
    }
    let store = Store::open(&directory.join("kokonut.sqlite3"))?;
    let boot = RefCell::new(Some((instance, store)));
    #[cfg(target_os = "windows")]
    let font = iced::Font::with_name("Segoe UI");
    #[cfg(target_os = "macos")]
    let font = iced::Font::with_name(".AppleSystemUIFont");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let font = iced::Font::DEFAULT;
    tracing::info!("Iniciando Kokonut {}", env!("CARGO_PKG_VERSION"));
    iced::daemon(
        move || {
            let (instance, store) = boot.borrow_mut().take().expect("boot runs once");
            App::boot(instance, store)
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .theme(App::theme)
    .subscription(App::subscription)
    .default_font(font)
    .style(|app, _| {
        let p = app.palette();
        iced::theme::Style {
            background_color: iced::Color::TRANSPARENT,
            text_color: p.text,
        }
    })
    .run()?;
    Ok(())
}
