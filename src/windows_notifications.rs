//! Desktop identity and native toast delivery. A successful Show call alone is not delivery proof.
use crate::{app::DATA_DIR, platform::NativeEvent};
use kokonut_calendar::model::Reminder;
use kokonut_calendar::sound_settings::SoundKind;
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::TypedEventHandler,
    UI::Notifications::{
        NotificationSetting, ToastDismissalReason, ToastDismissedEventArgs, ToastFailedEventArgs,
        ToastNotification, ToastNotificationManager,
    },
    Win32::{
        Storage::EnhancedStorage::PKEY_AppUserModel_ID,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
                CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
                StructuredStorage::{PROPVARIANT, PVCHF_DEFAULT, PropVariantChangeType},
            },
            Variant::VT_LPWSTR,
        },
        UI::Shell::{
            IShellLinkW, PropertiesSystem::IPropertyStore, SetCurrentProcessExplicitAppUserModelID,
            ShellLink,
        },
    },
    core::{HSTRING, Interface, PCWSTR, w},
};

pub const APP_ID: &str = "Kokonut.Calendar";
// Retain event handlers for notifications still in Action Center; cap memory independently of history.
static ACTIVE: Mutex<VecDeque<ToastNotification>> = Mutex::new(VecDeque::new());

struct Apartment;
impl Apartment {
    fn new(sta: bool) -> windows::core::Result<Self> {
        // Every call is on a fresh worker thread. Balance every successful initialization.
        unsafe {
            CoInitializeEx(
                None,
                if sta {
                    COINIT_APARTMENTTHREADED
                } else {
                    COINIT_MULTITHREADED
                },
            )
            .ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub fn register(directory: &Path) -> Result<(), String> {
    // Give the process, shortcut, toast notifier and registry the same explicit identity.
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(w!("Kokonut.Calendar"))
            .map_err(|e| e.to_string())?;
    }
    let ico = directory.join("kokonut.ico");
    let png = directory.join("kokonut.png");
    std::fs::write(&ico, include_bytes!("../assets/kokonut.ico")).map_err(|e| e.to_string())?;
    std::fs::write(&png, include_bytes!("../assets/kokonut.png")).map_err(|e| e.to_string())?;
    let key = windows_registry::CURRENT_USER
        .create("Software\\Classes\\AppUserModelId\\Kokonut.Calendar")
        .map_err(|e| e.to_string())?;
    key.set_string("DisplayName", "Kokonut Calendar")
        .map_err(|e| e.to_string())?;
    key.set_string("IconUri", png.to_string_lossy())
        .map_err(|e| e.to_string())?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let programs = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .ok_or("No se encontró el menú Inicio del usuario.")?
        .join("Microsoft/Windows/Start Menu/Programs");
    std::fs::create_dir_all(&programs).map_err(|e| e.to_string())?;
    let shortcut = programs.join("Kokonut Calendar.lnk");
    std::thread::spawn(move || -> Result<(), String> {
        let _apartment = Apartment::new(true).map_err(|e| e.to_string())?;
        let target = HSTRING::from(executable.as_os_str());
        let path = HSTRING::from(shortcut.as_os_str());
        let icon = HSTRING::from(ico.as_os_str());
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
            link.SetPath(PCWSTR(target.as_ptr()))
                .map_err(|e| e.to_string())?;
            link.SetDescription(w!("Calendario y recordatorios de Kokonut"))
                .map_err(|e| e.to_string())?;
            link.SetIconLocation(PCWSTR(icon.as_ptr()), 0)
                .map_err(|e| e.to_string())?;
            let properties: IPropertyStore = link.cast().map_err(|e| e.to_string())?;
            let source = PROPVARIANT::from(APP_ID);
            let mut id = PROPVARIANT::default();
            PropVariantChangeType(&mut id, &source, PVCHF_DEFAULT, VT_LPWSTR)
                .map_err(|e| e.to_string())?;
            properties
                .SetValue(&PKEY_AppUserModel_ID, &id)
                .map_err(|e| e.to_string())?;
            properties.Commit().map_err(|e| e.to_string())?;
            let file: IPersistFile = link.cast().map_err(|e| e.to_string())?;
            file.Save(PCWSTR(path.as_ptr()), true)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .join()
    .map_err(|_| "No se pudo registrar el acceso directo de Kokonut.".to_string())??;
    Ok(())
}

fn availability(setting: NotificationSetting) -> Result<(), String> {
    match setting{
        NotificationSetting::Enabled=>Ok(()),
        NotificationSetting::DisabledForApplication=>Err("Windows tiene desactivadas las notificaciones de Kokonut. Revisa Configuración → Sistema → Notificaciones → Kokonut Calendar.".into()),
        NotificationSetting::DisabledForUser=>Err("Las notificaciones de Windows están desactivadas para este usuario.".into()),
        NotificationSetting::DisabledByGroupPolicy=>Err("Una directiva del sistema bloquea las notificaciones.".into()),
        NotificationSetting::DisabledByManifest=>Err("Windows no reconoce correctamente el registro de notificaciones de Kokonut.".into()),
        _=>Err("Windows no permite mostrar esta notificación.".into()),
    }
}

fn banners_enabled() -> Result<(), String> {
    if let Ok(settings) = windows_registry::CURRENT_USER.open(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Notifications\\Settings\\Kokonut.Calendar",
    ) && settings.get_u32("ShowBanner").ok() == Some(0)
    {
        return Err("Los banners de Kokonut están desactivados en Windows. Actívalos en Configuración → Sistema → Notificaciones → Kokonut Calendar.".into());
    }
    Ok(())
}

pub fn check_permission() -> Result<(), String> {
    std::thread::spawn(|| {
        let _apartment = Apartment::new(false).map_err(|e| e.to_string())?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
            .map_err(|e| e.to_string())?;
        availability(notifier.Setting().map_err(|e| e.to_string())?)?;
        banners_enabled()
    })
    .join()
    .map_err(|_| "No se pudo comprobar el permiso de notificaciones.".to_string())?
}

fn status(sender: &UnboundedSender<NativeEvent>, result: Result<String, String>) {
    // No meeting titles or message content in diagnostics.
    if let Some(directory) = DATA_DIR.get() {
        let data = serde_json::json!({"at":chrono::Local::now().to_rfc3339(),"result":result});
        let _ = std::fs::write(
            directory.join("notification-diagnostic.json"),
            serde_json::to_vec_pretty(&data).unwrap_or_default(),
        );
    }
    let _ = sender.send(NativeEvent::NotificationResult(result));
}

pub fn notify(reminders: Vec<Reminder>, sender: UnboundedSender<NativeEvent>, kind: SoundKind) {
    std::thread::spawn(move || {
        let result = show(reminders, &sender, kind);
        if let Err(error) = result {
            status(&sender, Err(error));
        }
    });
}

fn show(
    reminders: Vec<Reminder>,
    sender: &UnboundedSender<NativeEvent>,
    kind: SoundKind,
) -> Result<(), String> {
    let _apartment = Apartment::new(false).map_err(|e| e.to_string())?;
    let Some(first) = reminders.first() else {
        return Ok(());
    };
    let app_id = HSTRING::from(APP_ID);
    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&app_id).map_err(|e| e.to_string())?;
    availability(notifier.Setting().map_err(|e| e.to_string())?)?;
    banners_enabled()?;
    let multiple = reminders.len() > 1;
    let title = if multiple {
        format!("{} recordatorios pendientes", reminders.len())
    } else {
        first.title.clone()
    };
    let body = if multiple {
        reminders
            .iter()
            .map(|r| r.title.as_str())
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        first.body.clone()
    };
    // Kokonut plays the sound belonging to this notification after delivery is confirmed.
    // Disable the Windows chime to avoid two audio tracks for the same banner.
    let xml = format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual><audio silent=\"true\"/></toast>",
        escape(&title),
        escape(&body)
    );
    let document = XmlDocument::new().map_err(|e| e.to_string())?;
    document
        .LoadXml(&HSTRING::from(xml))
        .map_err(|e| e.to_string())?;
    let toast = ToastNotification::CreateToastNotification(&document).map_err(|e| e.to_string())?;
    let tag = HSTRING::from(format!("{:x}", first.id));
    toast.SetTag(&tag).map_err(|e| e.to_string())?;
    let group = HSTRING::from(match kind {
        SoundKind::Notification => "kokonut",
        SoundKind::Advance => "advance",
    });
    toast.SetGroup(&group).map_err(|e| e.to_string())?;
    toast.SetSuppressPopup(false).map_err(|e| e.to_string())?;
    let event_id = if multiple {
        None
    } else {
        first.event_id.clone()
    };
    let acknowledged = Arc::new(AtomicBool::new(false));
    let acknowledged_by_user = acknowledged.clone();
    let tx = sender.clone();
    toast
        .Activated(&TypedEventHandler::new(
            move |_, _: windows::core::Ref<'_, windows::core::IInspectable>| {
                acknowledged_by_user.store(true, Ordering::SeqCst);
                status(&tx, Ok("Notificación abierta por el usuario".into()));
                let _ = tx.send(NativeEvent::Event(event_id.clone()));
                Ok(())
            },
        ))
        .map_err(|e| e.to_string())?;
    let failed = Arc::new(AtomicBool::new(false));
    let failure_flag = failed.clone();
    let tx = sender.clone();
    toast
        .Failed(&TypedEventHandler::new(
            move |_, args: windows::core::Ref<'_, ToastFailedEventArgs>| {
                failure_flag.store(true, Ordering::SeqCst);
                let code = args
                    .as_ref()
                    .and_then(|a| a.ErrorCode().ok())
                    .map(|c| format!("0x{:08X}", c.0 as u32))
                    .unwrap_or_else(|| "desconocido".into());
                status(
                    &tx,
                    Err(format!("Windows rechazó la notificación ({code}).")),
                );
                Ok(())
            },
        ))
        .map_err(|e| e.to_string())?;
    let tx = sender.clone();
    toast
        .Dismissed(&TypedEventHandler::new(
            move |_, args: windows::core::Ref<'_, ToastDismissedEventArgs>| {
                if args.as_ref().and_then(|a| a.Reason().ok())
                    == Some(ToastDismissalReason::TimedOut)
                {
                    status(
                        &tx,
                        Ok("Windows informó el cierre del aviso por tiempo".into()),
                    );
                }
                Ok(())
            },
        ))
        .map_err(|e| e.to_string())?;
    notifier.Show(&toast).map_err(|e| e.to_string())?;
    let mut active = ACTIVE.lock().map_err(|e| e.to_string())?;
    active.push_back(toast);
    while active.len() > 32 {
        active.pop_front();
    }
    drop(active);
    // Check this exact toast in our own history, rather than treating Show(Ok) as success.
    let mut stored = false;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(100));
        if failed.load(Ordering::SeqCst) {
            return Ok(());
        }
        if acknowledged.load(Ordering::SeqCst) {
            let _ = sender.send(NativeEvent::NotificationSound(kind));
            return Ok(());
        }
        let history = ToastNotificationManager::History()
            .and_then(|h| h.GetHistoryWithId(&app_id))
            .map_err(|e| e.to_string())?;
        for index in 0..history.Size().map_err(|e| e.to_string())? {
            if let Ok(item) = history.GetAt(index)
                && item.Tag().ok().as_ref() == Some(&tag)
                && item.Group().ok().as_ref() == Some(&group)
            {
                stored = true;
                break;
            }
        }
        if stored {
            let _ = sender.send(NativeEvent::NotificationSound(kind));
            break;
        }
    }
    if !failed.load(Ordering::SeqCst) {
        if stored {
            status(
                sender,
                Ok("Notificación confirmada en el centro de notificaciones de Windows".into()),
            );
        } else {
            return Err(
                "Windows aceptó el envío, pero no confirmó el aviso en su historial.".into(),
            );
        }
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_text_cannot_inject_toast_markup() {
        assert_eq!(escape("<hello> & \"'"), "&lt;hello&gt; &amp; &quot;&apos;");
    }
    #[test]
    fn disabled_notifications_are_not_reported_as_delivered() {
        assert!(availability(NotificationSetting::Enabled).is_ok());
        for setting in [
            NotificationSetting::DisabledForApplication,
            NotificationSetting::DisabledForUser,
            NotificationSetting::DisabledByGroupPolicy,
            NotificationSetting::DisabledByManifest,
        ] {
            assert!(availability(setting).is_err());
        }
    }
}
