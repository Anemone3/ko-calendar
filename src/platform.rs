#[cfg(target_os = "macos")]
use crate::icons::tray_template_pixels;
use crate::icons::{TRAY_SIZE, tray_pixels};
use iced::{
    Subscription,
    futures::{SinkExt, Stream},
    stream,
};
use kokonut_calendar::model::{Reminder, valid_web_url};
use kokonut_calendar::sound_settings::SoundKind;
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
#[derive(Debug, Clone)]
pub enum NativeEvent {
    Open,
    Test,
    Settings,
    Quit,
    Event(Option<String>),
    NotificationResult(Result<String, String>),
    NotificationSound(SoundKind),
}
static RECEIVER: OnceLock<Mutex<Option<UnboundedReceiver<NativeEvent>>>> = OnceLock::new();
pub struct Native {
    pub sender: UnboundedSender<NativeEvent>,
    pub tray: Option<TrayIcon>,
    pub tray_error: Option<String>,
}
impl Native {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded_channel();
        let _ = RECEIVER.set(Mutex::new(Some(receiver)));
        let menu = Menu::new();
        let mut mappings = Vec::new();
        for (label, event) in [
            ("Abrir calendario", NativeEvent::Open),
            ("Configuración", NativeEvent::Settings),
            ("Probar notificación", NativeEvent::Test),
            ("Salir", NativeEvent::Quit),
        ] {
            let item = MenuItem::new(label, true, None);
            mappings.push((item.id().clone(), event));
            let _ = menu.append(&item);
        }
        let tx = sender.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, action)) = mappings.iter().find(|(id, _)| *id == event.id) {
                let _ = tx.send(action.clone());
            }
        }));
        let tx = sender.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = tx.send(NativeEvent::Open);
            }
        }));
        // macOS menu bar: monochrome template the system tints; elsewhere the colour melon.
        #[cfg(target_os = "macos")]
        let pixels = tray_template_pixels();
        #[cfg(not(target_os = "macos"))]
        let pixels = tray_pixels();
        let size = TRAY_SIZE as u32;
        let result = tray_icon::Icon::from_rgba(pixels, size, size)
            .map_err(|e| e.to_string())
            .and_then(|icon| {
                let builder = TrayIconBuilder::new().with_id("ko-calendar");
                // StatusNotifierItem hosts show the title and append the tooltip
                // ("ko-calendar • …"), so Linux sets only the title. Windows and
                // macOS show the tooltip on hover (a macOS title would be drawn
                // next to the icon in the menu bar).
                #[cfg(target_os = "linux")]
                let builder = builder.with_title("ko-calendar");
                #[cfg(not(target_os = "linux"))]
                let builder = builder.with_tooltip("ko-calendar");
                #[cfg(target_os = "macos")]
                let builder = builder.with_icon_templated(icon);
                #[cfg(not(target_os = "macos"))]
                let builder = builder.with_icon(icon);
                builder
                    .with_menu(Box::new(menu))
                    .with_menu_on_left_click(false)
                    .build()
                    .map_err(|e| e.to_string())
            });
        let (tray, tray_error) = match result {
            Ok(tray) => (Some(tray), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            sender,
            tray,
            tray_error,
        }
    }
}
pub fn subscription() -> Subscription<NativeEvent> {
    Subscription::run(events)
}
fn events() -> impl Stream<Item = NativeEvent> {
    stream::channel(
        16,
        |mut output: iced::futures::channel::mpsc::Sender<NativeEvent>| async move {
            let receiver = RECEIVER.get().and_then(|lock| lock.lock().ok()?.take());
            if let Some(mut receiver) = receiver {
                while let Some(event) = receiver.recv().await {
                    if output.send(event).await.is_err() {
                        break;
                    }
                }
            }
            std::future::pending::<()>().await;
        },
    )
}
pub fn notify(reminders: Vec<Reminder>, sender: UnboundedSender<NativeEvent>, kind: SoundKind) {
    #[cfg(target_os = "windows")]
    crate::windows_notifications::notify(reminders, sender, kind);
    #[cfg(not(target_os = "windows"))]
    std::thread::spawn(move || {
        if let Some(first) = reminders.first() {
            let title = if reminders.len() > 1 {
                format!("{} eventos", reminders.len())
            } else {
                first.title.clone()
            };
            let mut notification = notify_rust::Notification::new();
            notification
                .appname(kokonut_calendar::APP_NAME)
                .summary(&title)
                .body(&first.body);
            #[cfg(target_os = "linux")]
            notification.hint(notify_rust::Hint::SuppressSound(true));
            match notification.show() {
                Ok(handle) => {
                    let _ = sender.send(NativeEvent::NotificationSound(kind));
                    let _ = sender.send(NativeEvent::NotificationResult(Ok(
                        "Aviso enviado al sistema".into(),
                    )));
                    let _ =
                        handle.wait_for_response(|response: &notify_rust::NotificationResponse| {
                            if matches!(response, notify_rust::NotificationResponse::Default) {
                                let _ = sender.send(NativeEvent::Event(first.event_id.clone()));
                            }
                        });
                }
                Err(error) => {
                    let _ = sender.send(NativeEvent::NotificationResult(Err(error.to_string())));
                }
            }
        }
    });
}
pub fn register_notifications(directory: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    return crate::windows_notifications::register(directory);
    #[cfg(not(target_os = "windows"))]
    {
        let _ = directory;
        Ok(())
    }
}
pub fn check_notification_permission() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    return crate::windows_notifications::check_permission();
    #[cfg(not(target_os = "windows"))]
    Ok(())
}
pub fn open_notification_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .arg("ms-settings:notifications")
            .creation_flags(0x08000000)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    Err("Revisa las notificaciones en los ajustes de tu escritorio.".into())
}
pub fn open_link(url: &str) -> Result<(), String> {
    if !valid_web_url(url) {
        return Err("El evento no tiene un enlace web válido.".into());
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("rundll32.exe");
        c.args(["url.dll,FileProtocolHandler", url]);
        c.creation_flags(0x08000000);
        c
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn open_sound_directory(directory: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("explorer.exe");
        c.creation_flags(0x08000000);
        c
    };
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    command
        .arg(directory)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
