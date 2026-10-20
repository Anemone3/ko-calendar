use crate::{
    icons::{TRAY_SIZE, tray_pixels},
    platform::{self, Native, NativeEvent},
};
use chrono::{Duration, Local, NaiveDate};
use iced::{Element, Size, Subscription, Task, Theme, event, keyboard, window};
use kokonut_calendar::{
    calendar,
    google::{self, Session, Snapshot},
    model::{Event, Reminder},
    sound_settings::{SoundKind, SoundSettings, without_duplicate_notifications},
    store::Store,
};
use std::{fs::File, sync::OnceLock};
pub static DATA_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GooglePhase {
    Loading,
    Idle,
    Authorizing,
    Saving,
    Syncing,
    Disconnecting,
}
#[derive(Debug, Clone)]
pub enum Message {
    Native(NativeEvent),
    Opened,
    Close(window::Id),
    Closed(window::Id),
    Navigate(i32),
    Today,
    Expand,
    SelectDate(NaiveDate),
    CloseAgenda,
    Detail(String),
    DetailMeasured(String, f32),
    Back,
    EventAction(String),
    OpenLink(String),
    Open,
    Hide,
    Quit,
    Drag,
    Escape,
    Test,
    OpenNotificationSettings,
    DismissNotice,
    Wake(u64),
    Minute,
    Settings,
    CloseSettings,
    SoundEnabled(bool),
    SoundFile(SoundKind, String),
    AdvanceEnabled(bool),
    SoundVolume(u8),
    SaveSound,
    SoundPreview(SoundKind),
    SoundStop,
    SoundFolder,
    SoundCompleted(u64, SoundKind, Result<(), String>),
    GoogleConnect,
    GoogleCancel,
    GoogleSync,
    GoogleDisconnect,
    GoogleLoaded(u64, Result<Option<Session>, String>),
    GoogleAuthorized(u64, Result<Session, String>),
    GoogleSaved(u64, Result<Session, String>),
    GoogleSynced(u64, Result<(Session, Snapshot), String>),
    GoogleDisconnected(u64, Result<Option<String>, String>),
}
pub struct App {
    pub store: Store,
    pub native: Native,
    pub main_window: Option<window::Id>,
    pub selected: NaiveDate,
    pub today: NaiveDate,
    pub expanded: bool,
    pub agenda_open: bool,
    pub detail: Option<String>,
    pub detail_header: Option<(String, f32)>,
    pub detail_generation: u64,
    pub events: Vec<Event>,
    pub notification_error: Option<String>,
    pub notification_status: Option<Result<String, String>>,
    pub generation: u64,
    pub settings_open: bool,
    pub google_phase: GooglePhase,
    pub google_session: Option<Session>,
    pub google_snapshot: Option<Snapshot>,
    pub google_error: Option<String>,
    pub sound_settings: SoundSettings,
    pub sound_files: Vec<String>,
    pub sound_error: Option<String>,
    pub sound_playing: bool,
    pub sound_last_result: Option<Result<(), String>>,
    pub sound_kind: Option<SoundKind>,
    pub sound_last_kind: Option<SoundKind>,
    sound_queue: std::collections::VecDeque<SoundKind>,
    sound_player: crate::sound::Player,
    sound_job: u64,
    google_generation: u64,
    google_abort: Option<iced::task::Handle>,
    pub _instance: File,
}
impl App {
    pub fn boot(instance: File, mut store: Store) -> (Self, Task<Message>) {
        let today = Local::now().date_naive();
        let registration =
            platform::register_notifications(DATA_DIR.get().expect("data directory"));
        let notification_error = registration
            .and_then(|_| platform::check_notification_permission())
            .err();
        let native = Native::new();
        let snapshot = store.google_snapshot().ok().flatten();
        let mut sound_error = None;
        let sound_settings = store.sound_settings().unwrap_or_else(|error| {
            sound_error = Some(format!("No se pudo cargar el sonido: {error}"));
            SoundSettings::default()
        });
        let sound_files =
            crate::sound::initialize(DATA_DIR.get().unwrap()).unwrap_or_else(|error| {
                sound_error = Some(error);
                vec![]
            });
        if let Some(snapshot) = &snapshot
            && let Err(error) = store.reconcile_alarms(snapshot, Local::now().timestamp())
        {
            sound_error = Some(format!("No se pudieron preparar las alarmas: {error}"));
        }
        // Without a synced Google calendar the view stays empty: no sample events.
        let mut events: Vec<Event> = snapshot
            .as_ref()
            .map(|snapshot| snapshot.events.iter().map(|e| e.event.clone()).collect())
            .unwrap_or_default();
        events.sort_by_key(|e| (e.date, e.start));
        let mut app = Self {
            store,
            native,
            main_window: None,
            selected: today,
            today,
            expanded: false,
            agenda_open: false,
            detail: None,
            detail_header: None,
            detail_generation: 0,
            events,
            notification_error,
            notification_status: None,
            generation: 0,
            settings_open: std::env::args().any(|a| a == "--settings"),
            google_phase: GooglePhase::Loading,
            google_session: None,
            google_snapshot: snapshot,
            google_error: None,
            sound_settings,
            sound_files,
            sound_error,
            sound_playing: false,
            sound_last_result: None,
            sound_kind: None,
            sound_last_kind: None,
            sound_queue: std::collections::VecDeque::new(),
            sound_player: crate::sound::Player::default(),
            sound_job: 0,
            google_generation: 0,
            google_abort: None,
            _instance: instance,
        };
        let hidden = std::env::args().any(|a| a == "--tray")
            && app.native.tray.is_some()
            && app.notification_error.is_none();
        let open = if hidden { Task::none() } else { app.open() };
        let wake = app.arm_scheduler();
        let load = Task::perform(google::load(DATA_DIR.get().unwrap().clone()), |result| {
            Message::GoogleLoaded(0, result)
        });
        let tasks = Task::batch([open, wake, load]);
        let tasks = if std::env::args().any(|a| a == "--test-notification") {
            Task::batch([tasks, app.update(Message::Test)])
        } else {
            tasks
        };
        (app, tasks)
    }
    pub fn title(&self, _: window::Id) -> String {
        kokonut_calendar::APP_NAME.into()
    }
    pub fn theme(&self, _: window::Id) -> Theme {
        let p = self.palette();
        Theme::custom(
            kokonut_calendar::APP_NAME,
            iced::theme::Palette {
                background: p.background,
                text: p.text,
                primary: p.accent,
                success: p.accent,
                warning: p.accent,
                danger: p.error,
            },
        )
    }
    pub fn view(&self, _: window::Id) -> Element<'_, Message> {
        self.main_view()
    }
    pub fn panel_size(&self) -> Size {
        let width = crate::ui::PANEL_WIDTH;
        if self.notification_error.is_some() {
            return Size::new(width, 300.);
        }
        if self.settings_open {
            let height = if self.google_session.is_some() {
                265.
            } else {
                230.
            };
            return Size::new(
                width,
                height
                    + 170.
                    + if self.sound_settings.advance_enabled {
                        65.
                    } else {
                        0.
                    }
                    + if self.google_error.is_some() { 96. } else { 0. }
                    + if self.sound_error.is_some() { 64. } else { 0. },
            );
        }
        let base = if self.expanded {
            86. + calendar::month_days(self.selected, true).len() as f32 / 7. * 36.
        } else {
            crate::ui::WEEK_HEIGHT
        };
        let extra = if let Some(event) = self
            .detail
            .as_ref()
            .and_then(|id| self.events.iter().find(|event| &event.id == id))
        {
            if let Some((_, height)) = self
                .detail_header
                .as_ref()
                .filter(|(id, _)| id == &event.id)
            {
                23. + height
                    + if event.description.trim().is_empty() {
                        0.
                    } else {
                        12. + crate::ui::DESCRIPTION_MIN_HEIGHT
                    }
            } else {
                354.
            }
        } else if self.agenda_open {
            68. + self
                .events
                .iter()
                .filter(|e| e.occurs_on(self.selected))
                .count()
                .clamp(1, 4) as f32
                * 62.
        } else {
            0.
        };
        let tray_notice = if self.native.tray_error.is_some() {
            100.
        } else {
            0.
        };
        Size::new(width, (base + extra + tray_notice).min(700.))
    }
    fn resize(&self) -> Task<Message> {
        let Some(id) = self.main_window else {
            return Task::none();
        };
        let size = self.panel_size();
        // On Linux winit pins a non-resizable window with min = max = its initial
        // size and never updates those hints on resize. Floating Wayland/X11
        // windows (Hyprland included) clamp to them, so the panel stayed at
        // 352 × 126. Move both hints with the panel: release the minimum first so
        // min <= max holds at every step, then pin the new size.
        if cfg!(target_os = "linux") {
            window::set_min_size(id, None)
                .chain(window::set_max_size(id, Some(size)))
                .chain(window::set_min_size(id, Some(size)))
                .chain(window::resize(id, size))
        } else {
            window::resize(id, size)
        }
    }
    fn open(&mut self) -> Task<Message> {
        if let Some(id) = self.main_window {
            return Task::batch([self.resize(), window::gain_focus(id)]);
        }
        let settings = window::Settings {
            size: self.panel_size(),
            position: window::Position::Centered,
            resizable: false,
            decorations: false,
            transparent: true,
            minimizable: false,
            exit_on_close_request: false,
            icon: window::icon::from_rgba(tray_pixels(), TRAY_SIZE as u32, TRAY_SIZE as u32).ok(),
            // Wayland app_id: matches packaging/linux/ko-calendar.desktop so the
            // compositor can associate the icon and window rules with the panel.
            #[cfg(target_os = "linux")]
            platform_specific: window::settings::PlatformSpecific {
                application_id: "ko-calendar".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let (id, open) = window::open(settings);
        self.main_window = Some(id);
        open.map(|_| Message::Opened)
    }
    fn hide(&mut self) -> Task<Message> {
        if self.native.tray.is_none() {
            return Task::none();
        }
        self.main_window
            .take()
            .map_or_else(Task::none, window::close)
    }
    fn arm_scheduler(&mut self) -> Task<Message> {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let now = Local::now().timestamp();
        let target = self.store.next_due().ok().flatten().unwrap_or(now + 60);
        Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_secs(
                    (target - now).clamp(1, 60) as u64
                ))
                .await;
                generation
            },
            Message::Wake,
        )
    }
    fn sync_google(&mut self) -> Task<Message> {
        if self.google_phase != GooglePhase::Idle {
            return Task::none();
        }
        let Some(session) = self.google_session.clone() else {
            return Task::none();
        };
        self.google_phase = GooglePhase::Syncing;
        self.google_generation = self.google_generation.wrapping_add(1);
        let generation = self.google_generation;
        let (task, handle) = Task::perform(google::sync(session), move |result| {
            Message::GoogleSynced(generation, result)
        })
        .abortable();
        self.google_abort = Some(handle);
        task
    }
    fn play_sound(&mut self, kind: SoundKind) -> Task<Message> {
        if !self.sound_settings.enabled_for(kind) || self.sound_settings.volume == 0 {
            return Task::none();
        }
        if self.sound_playing {
            if self.sound_kind != Some(kind) && !self.sound_queue.contains(&kind) {
                self.sound_queue.push_back(kind);
            }
            return Task::none();
        }
        let path = match crate::sound::file_path(
            DATA_DIR.get().unwrap(),
            self.sound_settings.file_for(kind),
        ) {
            Ok(path) => path,
            Err(error) => {
                self.sound_error = Some(error);
                return self.resize();
            }
        };
        self.sound_error = None;
        self.sound_playing = true;
        self.sound_kind = Some(kind);
        self.sound_last_result = None;
        let player = self.sound_player.clone();
        let job = player.next();
        self.sound_job = job;
        let volume = self.sound_settings.volume;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || player.play(path, volume, job))
                    .await
                    .map_err(|_| "No se pudo reproducir el sonido.".to_string())?
            },
            move |result| Message::SoundCompleted(job, kind, result),
        )
    }

    fn stop_sound(&mut self) {
        self.sound_job = self.sound_player.next();
        self.sound_playing = false;
        self.sound_kind = None;
    }
    fn next_sound(&mut self) -> Task<Message> {
        while let Some(kind) = self.sound_queue.pop_front() {
            if self.sound_settings.enabled_for(kind) {
                return self.play_sound(kind);
            }
        }
        Task::none()
    }
    fn cancel_sound(&mut self, kind: SoundKind) -> Task<Message> {
        self.sound_queue.retain(|queued| *queued != kind);
        if self.sound_kind == Some(kind) {
            self.stop_sound();
            return self.next_sound();
        }
        Task::none()
    }

    fn save_sound(&mut self) {
        self.sound_error = self
            .store
            .save_sound_settings(&self.sound_settings)
            .err()
            .map(|error| format!("No se pudo guardar el sonido: {error}"));
    }
    fn clear_google_view(&mut self) {
        self.google_snapshot = None;
        self.events.clear();
        self.detail = None;
        self.agenda_open = false;
    }
    pub fn subscription(&self) -> Subscription<Message> {
        let mut list = vec![
            platform::subscription().map(Message::Native),
            window::close_requests().map(Message::Close),
            window::close_events().map(Message::Closed),
            event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::Escape),
                _ => None,
            }),
        ];
        if self.main_window.is_some() {
            list.push(
                iced::time::every(std::time::Duration::from_secs(60)).map(|_| Message::Minute),
            );
        }
        if self.google_session.is_some() {
            list.push(
                iced::time::every(std::time::Duration::from_secs(300)).map(|_| Message::GoogleSync),
            );
        }
        Subscription::batch(list)
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Native(event) => match event {
                NativeEvent::NotificationSound(kind) => return self.play_sound(kind),
                NativeEvent::Open => return self.update(Message::Open),
                NativeEvent::Test => return self.update(Message::Test),
                NativeEvent::Settings => return self.update(Message::Settings),
                NativeEvent::Quit => return self.update(Message::Quit),
                NativeEvent::Event(event_id) => {
                    self.settings_open = false;
                    if let Some(event) =
                        event_id.and_then(|id| self.events.iter().find(|e| e.id == id))
                    {
                        self.selected = event.date;
                        self.agenda_open = true;
                        self.detail = Some(event.id.clone());
                        self.detail_header = None;
                        self.detail_generation = self.detail_generation.wrapping_add(1);
                    }
                    return self.open();
                }
                NativeEvent::NotificationResult(result) => {
                    if let Err(error) = &result {
                        self.notification_error = Some(error.clone());
                        self.notification_status = Some(result);
                        return self.open();
                    }
                    self.notification_status = Some(result);
                }
            },
            Message::Opened => {}
            Message::Closed(id) => {
                if self.main_window == Some(id) {
                    self.main_window = None;
                }
            }
            Message::Close(id) => {
                if self.main_window == Some(id) {
                    return self.hide();
                }
            }
            Message::Open => {
                self.selected = Local::now().date_naive();
                self.settings_open = false;
                self.expanded = false;
                self.agenda_open = false;
                self.detail = None;
                return self.open();
            }
            Message::Hide => return self.hide(),
            Message::Quit => {
                self.sound_player.stop();
                return iced::exit();
            }
            Message::Drag => return self.main_window.map_or_else(Task::none, window::drag),
            Message::Navigate(delta) => {
                self.selected = if self.expanded {
                    calendar::shift_month(self.selected, delta)
                } else {
                    self.selected + Duration::days(i64::from(delta) * 7)
                };
                self.agenda_open = false;
                self.detail = None;
            }
            Message::Today => {
                self.today = Local::now().date_naive();
                self.selected = self.today;
                self.detail = None;
            }
            Message::Expand => {
                self.expanded = !self.expanded;
                self.agenda_open = false;
                self.detail = None;
            }
            Message::SelectDate(date) => {
                self.agenda_open = self.expanded || date != self.selected || !self.agenda_open;
                self.selected = date;
                self.expanded = false;
                self.detail = None;
            }
            Message::CloseAgenda => {
                self.agenda_open = false;
                self.detail = None;
            }
            Message::Detail(id) => {
                self.detail = Some(id);
                self.detail_header = None;
                self.detail_generation = self.detail_generation.wrapping_add(1);
            }
            Message::DetailMeasured(id, height) => {
                if self.detail.as_ref() != Some(&id)
                    || !height.is_finite()
                    || height <= 0.
                    || self
                        .detail_header
                        .as_ref()
                        .is_some_and(|(previous_id, previous_height)| {
                            previous_id == &id && (previous_height - height.ceil()).abs() < 1.
                        })
                {
                    return Task::none();
                }
                // Wait for the new event's widgets to exist before resetting their scroll.
                let scroll = if self
                    .detail_header
                    .as_ref()
                    .is_none_or(|(old, _)| old != &id)
                {
                    iced::widget::operation::snap_to(
                        format!("description-{id}"),
                        iced::widget::operation::RelativeOffset::START,
                    )
                } else {
                    Task::none()
                };
                self.detail_header = Some((id, height.ceil()));
                return Task::batch([self.resize(), scroll]);
            }
            Message::Back => self.detail = None,
            Message::Escape => {
                if self.notification_error.is_some() {
                    self.notification_error = None;
                } else if self.settings_open {
                    self.settings_open = false;
                } else if self.detail.is_some() {
                    self.detail = None;
                } else if self.agenda_open {
                    self.agenda_open = false;
                } else if self.expanded {
                    self.expanded = false;
                } else {
                    return self.hide();
                }
            }
            Message::EventAction(id) => {
                if let Some(event) = self.events.iter().find(|e| e.id == id)
                    && !event.demo
                    && !event.destination.is_empty()
                    && let Err(error) = platform::open_link(&event.destination)
                {
                    self.notification_error = Some(error);
                }
            }
            Message::OpenLink(url) => {
                if let Err(error) = platform::open_link(&url) {
                    self.notification_error = Some(error);
                }
            }
            Message::Test => {
                self.notification_error = None;
                let registration =
                    platform::register_notifications(DATA_DIR.get().expect("data directory"))
                        .and_then(|_| platform::check_notification_permission());
                if let Err(error) = registration {
                    self.notification_error = Some(error);
                    return self.open();
                }
                if let Err(error) = self.store.schedule_test(
                    "Tu aviso de ko-calendar",
                    "Este es un aviso de prueba de ko-calendar.",
                    Local::now().timestamp() + 2,
                    false,
                    None,
                ) {
                    self.notification_error = Some(error.to_string());
                    return self.open();
                }
                return self.arm_scheduler();
            }
            Message::OpenNotificationSettings => {
                if let Err(error) = platform::open_notification_settings() {
                    self.notification_error = Some(error);
                }
            }
            Message::DismissNotice => self.notification_error = None,
            Message::Wake(generation) => {
                if generation != self.generation {
                    return Task::none();
                }
                let now = Local::now().timestamp();
                let alarms = match self.store.claim_alarms(now) {
                    Ok(due) => due,
                    Err(error) => {
                        self.sound_error = Some(format!("No se pudo comprobar la alarma: {error}"));
                        vec![]
                    }
                };
                let early: Vec<Reminder> = if self.sound_settings.advance_enabled {
                    alarms
                        .into_iter()
                        .filter_map(|alarm| {
                            self.events
                                .iter()
                                .find(|event| event.id == alarm.event_id)
                                .map(|event| Reminder {
                                    id: alarm.id,
                                    title: format!("En 2 minutos · {}", event.title),
                                    body: format!("Empieza a las {}", event.time_label(true)),
                                    due_at: alarm.due_at,
                                    important: false,
                                    state: "delivered".into(),
                                    event_id: Some(event.id.clone()),
                                })
                        })
                        .collect()
                } else {
                    vec![]
                };
                match self.store.claim_due(now) {
                    Ok(due) => {
                        let due = without_duplicate_notifications(due, &early);
                        if !due.is_empty() {
                            platform::notify(
                                due,
                                self.native.sender.clone(),
                                SoundKind::Notification,
                            );
                        }
                    }
                    Err(error) => {
                        self.notification_error = Some(error.to_string());
                        return Task::batch([self.open(), self.arm_scheduler()]);
                    }
                }
                if !early.is_empty() {
                    platform::notify(early, self.native.sender.clone(), SoundKind::Advance);
                }
                return self.arm_scheduler();
            }
            Message::Minute => {
                let today = Local::now().date_naive();
                if today != self.today {
                    self.today = today;
                }
                return Task::none();
            }
            Message::Settings => {
                self.settings_open = true;
                match crate::sound::initialize(DATA_DIR.get().unwrap()) {
                    Ok(files) => self.sound_files = files,
                    Err(error) => self.sound_error = Some(error),
                }
                return self.open();
            }
            Message::CloseSettings => {
                self.settings_open = false;
                self.expanded = false;
                self.agenda_open = false;
                self.detail = None;
            }
            Message::SoundEnabled(enabled) => {
                self.sound_settings.enabled = enabled;
                self.save_sound();
                if !enabled {
                    return self.cancel_sound(SoundKind::Notification);
                }
            }
            Message::AdvanceEnabled(enabled) => {
                self.sound_settings.advance_enabled = enabled;
                self.save_sound();
                if !enabled {
                    return Task::batch([self.cancel_sound(SoundKind::Advance), self.resize()]);
                }
            }
            Message::SoundFile(kind, file) => {
                if self.sound_files.contains(&file) {
                    match kind {
                        SoundKind::Notification => self.sound_settings.file = file,
                        SoundKind::Advance => self.sound_settings.advance_file = file,
                    }
                    self.save_sound();
                }
            }
            Message::SoundVolume(volume) => self.sound_settings.volume = volume.min(100),
            Message::SaveSound => self.save_sound(),
            Message::SoundPreview(kind) => {
                self.stop_sound();
                self.sound_queue.clear();
                return self.play_sound(kind);
            }
            Message::SoundStop => {
                self.stop_sound();
                self.sound_queue.clear();
            }
            Message::SoundFolder => {
                if let Err(error) = platform::open_sound_directory(&crate::sound::directory(
                    DATA_DIR.get().unwrap(),
                )) {
                    self.sound_error = Some(error);
                }
            }
            Message::SoundCompleted(job, kind, result) => {
                if job != self.sound_job {
                    return Task::none();
                }
                self.sound_playing = false;
                self.sound_kind = None;
                self.sound_error = result.as_ref().err().cloned();
                if result.is_ok() {
                    tracing::info!(?kind, "Sonido de aviso reproducido");
                }
                self.sound_last_result = Some(result);
                self.sound_last_kind = Some(kind);
                return Task::batch([self.next_sound(), self.resize()]);
            }
            Message::GoogleLoaded(generation, result) => {
                if generation != self.google_generation {
                    return Task::none();
                }
                self.google_phase = GooglePhase::Idle;
                match result {
                    Ok(Some(session)) => {
                        self.google_session = Some(session);
                        return self.sync_google();
                    }
                    Ok(None) => {
                        if let Err(error) = self.store.clear_google() {
                            self.google_error = Some(error.to_string());
                        }
                        self.clear_google_view();
                    }
                    Err(error) => self.google_error = Some(error),
                }
            }
            Message::GoogleConnect => {
                if self.google_phase != GooglePhase::Idle || self.google_session.is_some() {
                    return Task::none();
                }
                self.google_error = None;
                self.google_phase = GooglePhase::Authorizing;
                self.google_generation = self.google_generation.wrapping_add(1);
                let generation = self.google_generation;
                let (task, handle) =
                    Task::perform(google::connect(platform::open_link), move |r| {
                        Message::GoogleAuthorized(generation, r)
                    })
                    .abortable();
                self.google_abort = Some(handle);
                return task;
            }
            Message::GoogleCancel => {
                if self.google_phase == GooglePhase::Authorizing {
                    if let Some(handle) = self.google_abort.take() {
                        handle.abort();
                    }
                    self.google_generation = self.google_generation.wrapping_add(1);
                    self.google_phase = GooglePhase::Idle;
                    self.google_error = None;
                }
            }
            Message::GoogleAuthorized(generation, result) => {
                if generation != self.google_generation {
                    return Task::none();
                }
                self.google_abort = None;
                match result {
                    Ok(session) => {
                        self.google_phase = GooglePhase::Saving;
                        return Task::perform(
                            google::save(DATA_DIR.get().unwrap().clone(), session),
                            move |r| Message::GoogleSaved(generation, r),
                        );
                    }
                    Err(error) => {
                        self.google_phase = GooglePhase::Idle;
                        self.google_error = Some(error);
                    }
                }
            }
            Message::GoogleSaved(generation, result) => {
                if generation != self.google_generation {
                    return Task::none();
                }
                self.google_phase = GooglePhase::Idle;
                match result {
                    Ok(session) => {
                        self.google_session = Some(session);
                        self.settings_open = true;
                        return Task::batch([self.open(), self.sync_google()]);
                    }
                    Err(error) => self.google_error = Some(error),
                }
            }
            Message::GoogleSync => return self.sync_google(),
            Message::GoogleSynced(generation, result) => {
                if generation != self.google_generation {
                    return Task::none();
                }
                self.google_abort = None;
                self.google_phase = GooglePhase::Idle;
                match result {
                    Ok((session, snapshot)) => {
                        self.google_session = Some(session);
                        match self.store.apply_google(&snapshot, Local::now().timestamp()) {
                            Ok(()) => {
                                self.events =
                                    snapshot.events.iter().map(|e| e.event.clone()).collect();
                                self.google_snapshot = Some(snapshot);
                                self.google_error = None;
                                return Task::batch([self.resize(), self.arm_scheduler()]);
                            }
                            Err(error) => {
                                tracing::error!(%error, "No se pudo guardar el calendario de Google");
                                self.google_error = Some(
                                    "No se pudieron guardar los eventos en la base de datos local."
                                        .into(),
                                );
                            }
                        }
                    }
                    Err(error) => self.google_error = Some(error),
                }
            }
            Message::GoogleDisconnect => {
                if !matches!(self.google_phase, GooglePhase::Idle | GooglePhase::Syncing) {
                    return Task::none();
                }
                let Some(session) = self.google_session.clone() else {
                    return Task::none();
                };
                if let Err(error) = self.store.clear_google() {
                    self.google_error = Some(error.to_string());
                    return Task::none();
                }
                if let Some(handle) = self.google_abort.take() {
                    handle.abort();
                }
                self.google_generation = self.google_generation.wrapping_add(1);
                let generation = self.google_generation;
                self.google_phase = GooglePhase::Disconnecting;
                self.clear_google_view();
                return Task::batch([
                    self.arm_scheduler(),
                    Task::perform(
                        google::disconnect(DATA_DIR.get().unwrap().clone(), session),
                        move |r| Message::GoogleDisconnected(generation, r),
                    ),
                ]);
            }
            Message::GoogleDisconnected(generation, result) => {
                if generation != self.google_generation {
                    return Task::none();
                }
                self.google_phase = GooglePhase::Idle;
                match result {
                    Ok(warning) => {
                        self.google_session = None;
                        self.google_error = warning;
                    }
                    Err(error) => self.google_error = Some(error),
                }
            }
        }
        self.resize()
    }
}
