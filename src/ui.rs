use crate::{
    app::{App, GooglePhase, Message},
    icons::{Icon, icon, logo},
};
use chrono::{Datelike, Duration, Local};
use iced::{
    Alignment, Border, Color, Element, Fill, Font, Length, border, font,
    widget::{
        button, checkbox, column, container, mouse_area, pick_list, rich_text, row, rule,
        scrollable, sensor, slider, space, span, text, toggler, tooltip,
    },
};
use kokonut_calendar::{
    calendar::{self, date_label, month_name},
    model::{DescriptionPart, Event, EventKind, description_parts, web_domain},
    sound_settings::SoundKind,
};
pub const PANEL_WIDTH: f32 = 352.;
pub const WEEK_HEIGHT: f32 = 126.;
pub const DESCRIPTION_MIN_HEIGHT: f32 = 112.;
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub selected: Color,
    pub accent: Color,
    pub text: Color,
    pub secondary: Color,
    pub muted: Color,
    pub line: Color,
    pub error: Color,
}
fn hex(n: u32) -> Color {
    Color::from_rgb8((n >> 16) as u8, (n >> 8) as u8, n as u8)
}
fn medium() -> Font {
    Font {
        weight: font::Weight::Medium,
        ..Font::default()
    }
}
impl App {
    pub fn palette(&self) -> Palette {
        Palette {
            background: hex(0xFFFFFF),
            surface: hex(0xF5F6F7),
            selected: hex(0x202124),
            accent: hex(0x3468CF),
            text: hex(0x202124),
            secondary: hex(0x70757D),
            muted: hex(0x9DA2A9),
            line: hex(0xECEEF0),
            error: hex(0xB43B40),
        }
    }
    pub fn main_view(&self) -> Element<'_, Message> {
        let p = self.palette();
        let body = if let Some(error) = &self.notification_error {
            self.notification_notice(error)
        } else if self.settings_open {
            self.settings_view()
        } else {
            self.calendar_view()
        };
        let mut content = column![body].width(Fill).spacing(12);
        if let Some(error) = &self.native.tray_error {
            content = content
                .push(
                    text(format!("La bandeja no está disponible: {error}"))
                        .size(11)
                        .color(p.error),
                )
                .push(flat_button(
                    p,
                    text("Salir de ko-calendar").size(12),
                    Message::Quit,
                ));
        }
        let body: Element<'_, Message> = if self.settings_open {
            scrollable(content).into()
        } else {
            content.height(Fill).into()
        };
        container(body)
            .padding([12, 14])
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(p.background.into()),
                text_color: Some(p.text),
                border: Border {
                    color: hex(0xE4E7EB),
                    width: 1.,
                    radius: 14.into(),
                },
                ..Default::default()
            })
            .into()
    }
    fn calendar_view(&self) -> Element<'_, Message> {
        let p = self.palette();
        let title = row![
            logo(16.),
            text(month_name(self.selected))
                .size(14)
                .font(medium())
                .color(p.text),
            text(self.selected.year()).size(12).color(p.secondary),
            icon(
                if self.expanded { Icon::Up } else { Icon::Down },
                12.,
                p.secondary
            )
        ]
        .spacing(5)
        .align_y(Alignment::Center);
        let mut header = row![
            flat_button(p, title, Message::Expand),
            mouse_area(container(space()).width(Fill).height(27)).on_press(Message::Drag),
        ];
        if self.selected != Local::now().date_naive() {
            header = header.push(flat_button(
                p,
                text("Hoy").size(11).font(medium()).color(p.accent),
                Message::Today,
            ));
        }
        let header = header
            .push(
                row![
                    icon_button(
                        p,
                        Icon::Left,
                        if self.expanded {
                            "Mes anterior"
                        } else {
                            "Semana anterior"
                        },
                        Message::Navigate(-1)
                    ),
                    icon_button(
                        p,
                        Icon::Right,
                        if self.expanded {
                            "Mes siguiente"
                        } else {
                            "Semana siguiente"
                        },
                        Message::Navigate(1)
                    ),
                    icon_button(p, Icon::Settings, "Configuración", Message::Settings),
                    icon_button(p, Icon::Close, "Ocultar en bandeja", Message::Hide)
                ]
                .spacing(2),
            )
            .spacing(2)
            .height(27)
            .align_y(Alignment::Center);
        let mut panel = column![header, space().height(10)].spacing(0).width(Fill);
        if self.expanded {
            let mut names = row![].spacing(3);
            for name in ["L", "M", "X", "J", "V", "S", "D"] {
                names = names.push(
                    container(text(name).size(11).color(p.secondary))
                        .center_x(Fill)
                        .width(Length::FillPortion(1)),
                );
            }
            let mut grid = column![names, space().height(3)].spacing(4);
            for week in calendar::month_days(self.selected, true).chunks(7) {
                let mut r = row![].spacing(3);
                for &date in week {
                    r = r.push(self.day_button(date, false));
                }
                grid = grid.push(r);
            }
            panel = panel.push(grid);
        } else {
            let start = calendar::week_start(self.selected, false);
            let mut strip = row![].spacing(3);
            for i in 0..7 {
                strip = strip.push(self.day_button(start + Duration::days(i), true));
            }
            panel = panel.push(strip);
        }
        if self.agenda_open || self.detail.is_some() {
            panel = panel
                .push(space().height(13))
                .push(divider(p))
                .push(space().height(9));
            if let Some(event) = self
                .detail
                .as_ref()
                .and_then(|id| self.events.iter().find(|e| &e.id == id))
            {
                panel = panel.push(self.detail_view(event));
            } else {
                panel = panel.push(self.agenda_view());
            }
        }
        panel.height(Fill).into()
    }
    fn day_button(&self, date: chrono::NaiveDate, weekly: bool) -> Element<'_, Message> {
        let p = self.palette();
        let selected = date == self.selected;
        let today = date == Local::now().date_naive();
        let color = if selected {
            p.background
        } else if !weekly && date.month() != self.selected.month() {
            p.muted
        } else {
            p.text
        };
        let mut content = column![].spacing(3).align_x(Alignment::Center).width(Fill);
        if weekly {
            content = content.push(
                text(
                    ["Lun", "Mar", "Mié", "Jue", "Vie", "Sáb", "Dom"]
                        [date.weekday().num_days_from_monday() as usize],
                )
                .size(11)
                .color(if selected { hex(0xE1E2E5) } else { p.secondary }),
            );
        }
        content = content.push(
            text(date.day())
                .size(if weekly { 17 } else { 13 })
                .color(color),
        );
        let mut dots = row![].spacing(3);
        for _ in self.events.iter().filter(|e| e.occurs_on(date)).take(1) {
            dots =
                dots.push(container(space().width(3).height(3)).style(move |_| surface(color, 2.)));
        }
        content = content.push(container(dots).height(3).center_x(Fill));
        button(container(content).center_y(Fill))
            .on_press(Message::SelectDate(date))
            .padding(if weekly { [5, 0] } else { [2, 0] })
            .height(if weekly { 61 } else { 32 })
            .width(Length::FillPortion(1))
            .style(move |_, status| button::Style {
                background: if selected {
                    Some(p.selected.into())
                } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    Some(p.surface.into())
                } else {
                    None
                },
                text_color: color,
                border: Border {
                    radius: 9.into(),
                    width: if today && !selected { 1. } else { 0. },
                    color: p.line,
                },
                ..Default::default()
            })
            .into()
    }
    fn agenda_view(&self) -> Element<'_, Message> {
        let p = self.palette();
        let events = self
            .events
            .iter()
            .filter(|e| e.occurs_on(self.selected))
            .collect::<Vec<_>>();
        let header = row![
            text(capitalize(&date_label(self.selected)))
                .size(12)
                .font(medium())
                .width(Fill),
            icon_button(p, Icon::Up, "Contraer agenda", Message::CloseAgenda)
        ]
        .align_y(Alignment::Center);
        let mut content = column![].spacing(0);
        if events.is_empty() {
            content = content.push(
                container(
                    text("No hay eventos para este día.")
                        .size(12)
                        .color(p.secondary),
                )
                .padding([13, 1]),
            );
        }
        let has_demo = events.iter().any(|e| e.demo);
        for (index, event) in events.into_iter().enumerate() {
            if index > 0 {
                content = content.push(divider(p));
            }
            content = content.push(self.event_row(event));
        }
        if has_demo {
            content = content.push(
                text("Vista previa · Eventos de ejemplo")
                    .size(11)
                    .color(p.secondary),
            );
        }
        column![header, space().height(3), scrollable(content).height(Fill)]
            .height(Fill)
            .into()
    }
    fn event_row<'a>(&self, event: &'a Event) -> Element<'a, Message> {
        let p = self.palette();
        let times = text(event.time_label(true))
            .size(12)
            .color(p.secondary)
            .width(if event.all_day { 66 } else { 38 });
        let details = column![
            text(&event.title).size(13).font(medium()),
            row![
                icon(kind_icon(event.display_kind()), 12., p.secondary),
                text(event.display_kind().label())
                    .size(11)
                    .color(p.secondary)
            ]
            .spacing(5)
            .align_y(Alignment::Center)
        ]
        .spacing(3)
        .width(Fill);
        button(
            row![times, details, icon(Icon::Right, 14., p.secondary)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .on_press(Message::Detail(event.id.clone()))
        .padding([10, 1])
        .width(Fill)
        .style(move |_, status| button::Style {
            background: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Some(p.surface.into())
            } else {
                None
            },
            text_color: p.text,
            border: border::rounded(8),
            ..Default::default()
        })
        .into()
    }
    fn detail_view<'a>(&self, event: &'a Event) -> Element<'a, Message> {
        let p = self.palette();
        let mut content = column![
            row![
                flat_button(
                    p,
                    row![icon(Icon::Back, 14., p.secondary), text("Agenda").size(11)].spacing(5),
                    Message::Back
                ),
                space().width(Fill),
                icon_button(p, Icon::Up, "Contraer agenda", Message::CloseAgenda)
            ]
            .align_y(Alignment::Center),
            text(&event.title).size(17).font(medium()),
            row![
                icon(Icon::Clock, 15., p.secondary),
                column![
                    text(capitalize(&date_label(event.date))).size(12),
                    text(if event.all_day {
                        event.time_label(true)
                    } else {
                        format!("{} – {}", event.time_label(true), event.end_label(true))
                    })
                    .size(11)
                    .color(p.secondary)
                ]
                .spacing(3)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                icon(kind_icon(event.display_kind()), 15., p.secondary),
                text(event.display_kind().label()).size(12)
            ]
            .spacing(9)
            .align_y(Alignment::Center)
        ]
        .spacing(12);
        if let Some(synced) = self
            .google_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.events.iter().find(|e| e.event.id == event.id))
        {
            let times = synced
                .reminders
                .iter()
                .filter_map(|&timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
                .map(|time| {
                    let local = time.with_timezone(&Local);
                    local
                        .format(if local.date_naive() == event.date {
                            "%H:%M"
                        } else {
                            "%d/%m · %H:%M"
                        })
                        .to_string()
                })
                .collect::<Vec<_>>();
            let label = if times.is_empty() {
                "Sin avisos emergentes".into()
            } else {
                format!("Aviso: {}", times.join(", "))
            };
            let mut alerts = row![
                container(
                    row![
                        icon(Icon::Bell, 14., p.accent),
                        column![
                            text(label).size(12),
                            text("Según Google Calendar").size(11).color(p.secondary)
                        ]
                        .spacing(3)
                    ]
                    .spacing(8),
                )
                .width(Length::FillPortion(1))
            ]
            .spacing(12);
            if self.sound_settings.advance_enabled
                && !event.all_day
                && let Some(at) = synced
                    .starts_at
                    .or_else(|| event.timestamp())
                    .and_then(|start| {
                        start.checked_sub(kokonut_calendar::sound_settings::ADVANCE_SECONDS)
                    })
                    .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
            {
                let local = at.with_timezone(&Local);
                let time = local.format(if local.date_naive() == event.date {
                    "%H:%M"
                } else {
                    "%d/%m · %H:%M"
                });
                alerts = alerts.push(
                    container(
                        row![
                            icon(Icon::Sound, 14., p.accent),
                            column![
                                text(format!("Sonido: {time}")).size(12),
                                text("2 min antes").size(11).color(p.secondary)
                            ]
                            .spacing(3)
                        ]
                        .spacing(8),
                    )
                    .width(Length::FillPortion(1)),
                );
            }
            content = content.push(alerts);
        }
        if !event.demo
            && let Some(domain) = web_domain(&event.destination)
        {
            content = content.push(
                button(
                    row![
                        column![
                            text(event.display_kind().action()).size(13).font(medium()),
                            text(domain)
                                .size(11)
                                .color(hex(0xE5EDFF))
                                .wrapping(text::Wrapping::None)
                        ]
                        .spacing(4)
                        .width(Fill),
                        icon(Icon::External, 18., Color::WHITE)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::EventAction(event.id.clone()))
                .padding([10, 14])
                .width(Fill)
                .style(move |_, status| button::Style {
                    background: Some(
                        match status {
                            button::Status::Hovered => hex(0x2855B5),
                            button::Status::Pressed => hex(0x234BA1),
                            _ => p.accent,
                        }
                        .into(),
                    ),
                    text_color: Color::WHITE,
                    border: border::rounded(10),
                    ..Default::default()
                }),
            );
        } else if !event.destination.is_empty() {
            content = content.push(text(&event.destination).size(11).color(p.secondary));
        }
        let id = event.id.clone();
        let shown_id = id.clone();
        let header = sensor(content)
            .key((event.id.clone(), self.detail_generation))
            .on_show(move |size| Message::DetailMeasured(shown_id.clone(), size.height))
            .on_resize(move |size| Message::DetailMeasured(id.clone(), size.height));
        let mut body = column![header].spacing(12);
        if !event.description.trim().is_empty() {
            let spans = description_parts(&event.description)
                .into_iter()
                .map(|part| match part {
                    DescriptionPart::Text(value) => span(value),
                    DescriptionPart::Link { domain, url } => {
                        span(domain).color(p.accent).link(url.to_owned())
                    }
                })
                .collect::<Vec<_>>();
            body = body.push(
                scrollable(
                    container(
                        rich_text(spans)
                            .size(12)
                            .color(p.secondary)
                            .on_link_click(Message::OpenLink),
                    )
                    .padding([0, 8])
                    .width(Fill),
                )
                .id(format!("description-{}", event.id))
                .height(Fill),
            );
        }
        body.height(Fill).into()
    }
    fn notification_notice<'a>(&self, error: &'a str) -> Element<'a, Message> {
        let p = self.palette();
        column![row![icon(Icon::Bell,22.,p.accent),text("El aviso necesita atención").size(18).font(medium()).width(Fill),icon_button(p,Icon::Close,"Volver",Message::DismissNotice)].spacing(10).align_y(Alignment::Center),space().height(5),text(error).size(13).color(p.secondary),text("Revisa las notificaciones de ko-calendar en Windows y vuelve a probar desde la bandeja.").size(12).color(p.secondary),space().height(4),action(p,"Abrir ajustes de Windows",Message::OpenNotificationSettings),flat_button(p,text("Volver al calendario").size(12),Message::DismissNotice)].spacing(14).into()
    }
    fn settings_view(&self) -> Element<'_, Message> {
        let p = self.palette();
        let status = match self.google_phase {
            GooglePhase::Loading => "Cargando cuenta…",
            GooglePhase::Authorizing => "Completa la conexión en tu navegador.",
            GooglePhase::Saving => "Guardando tu sesión…",
            GooglePhase::Syncing => "Actualizando tu calendario…",
            GooglePhase::Disconnecting => "Desconectando…",
            GooglePhase::Idle if self.google_session.is_some() => "Conectado",
            GooglePhase::Idle => "Conecta tu calendario y recibe sus avisos.",
        };
        let mut content = column![
            row![
                icon_button(p, Icon::Back, "Volver a la semana", Message::CloseSettings),
                mouse_area(container(text("Configuración").size(14).font(medium())).width(Fill))
                    .on_press(Message::Drag),
                icon_button(p, Icon::Close, "Ocultar en bandeja", Message::Hide)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            space().height(2),
            row![
                container(text("G").size(16).font(medium()).color(p.text))
                    .center(31)
                    .style(move |_| surface(p.surface, 9.)),
                column![
                    text(
                        self.google_session
                            .as_ref()
                            .map_or("Conecta tu calendario", |s| s.account.as_str())
                    )
                    .size(12),
                    text(status).size(11).color(p.secondary)
                ]
                .spacing(3)
                .width(Fill)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(10)
        .width(Fill);
        if let Some(snapshot) = &self.google_snapshot {
            let time = chrono::DateTime::from_timestamp(snapshot.synced_at, 0)
                .map(|t| t.with_timezone(&Local).format("%d/%m · %H:%M").to_string())
                .unwrap_or_default();
            content = content.push(
                text(format!("Calendario principal · Actualizado {time}"))
                    .size(11)
                    .color(p.secondary),
            );
        }
        if self.google_phase == GooglePhase::Authorizing {
            content = content.push(action(p, "Cancelar", Message::GoogleCancel));
        } else {
            let message = if self.google_phase == GooglePhase::Idle {
                Some(if self.google_session.is_some() {
                    Message::GoogleSync
                } else {
                    Message::GoogleConnect
                })
            } else {
                None
            };
            let label = if self.google_phase == GooglePhase::Syncing {
                "Sincronizando…"
            } else if self.google_session.is_some() {
                "Sincronizar ahora"
            } else {
                "Google Sync"
            };
            content = content.push(action_enabled(p, label, message));
            if self.google_session.is_some()
                && matches!(self.google_phase, GooglePhase::Idle | GooglePhase::Syncing)
            {
                content = content.push(flat_button(
                    p,
                    text("Desconectar cuenta").size(11),
                    Message::GoogleDisconnect,
                ));
            }
        }
        if let Some(error) = &self.google_error {
            content = content.push(text(error).size(12).color(p.error));
        }
        content = content.push(divider(p));
        content = content
            .push(space().height(2))
            .push(
                row![
                    text("Sonido de notificación").size(13).font(medium()),
                    space().width(Fill),
                    toggler(self.sound_settings.enabled)
                        .on_toggle(Message::SoundEnabled)
                        .size(18)
                ]
                .align_y(Alignment::Center),
            )
            .push(
                text("Con los avisos de Google Calendar")
                    .size(11)
                    .color(p.secondary),
            );
        if self.sound_settings.enabled {
            content = content.push(self.sound_picker(SoundKind::Notification));
        }
        content = content.push(divider(p)).push(
            checkbox(self.sound_settings.advance_enabled)
                .label("Avisar 2 minutos antes")
                .on_toggle(Message::AdvanceEnabled)
                .size(17)
                .text_size(13),
        );
        if self.sound_settings.advance_enabled {
            content = content
                .push(
                    text("Aviso adicional con su propio sonido · Una vez")
                        .size(11)
                        .color(p.secondary),
                )
                .push(self.sound_picker(SoundKind::Advance));
        }
        if self.sound_settings.enabled || self.sound_settings.advance_enabled {
            content = content.push(
                row![
                    text("Volumen").size(11).color(p.secondary),
                    slider(0..=100, self.sound_settings.volume, Message::SoundVolume)
                        .on_release(Message::SaveSound)
                        .width(Fill),
                    text(format!("{} %", self.sound_settings.volume))
                        .size(11)
                        .color(p.secondary)
                        .width(36)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
        content = content.push(flat_button(
            p,
            text("Carpeta de sonidos · MP3, WAV y más").size(11),
            Message::SoundFolder,
        ));
        if let Some(error) = &self.sound_error {
            content = content.push(text(error).size(11).color(p.error));
        }
        content.into()
    }
    fn sound_picker(&self, kind: SoundKind) -> Element<'_, Message> {
        let p = self.palette();
        let playing = self.sound_kind == Some(kind);
        row![
            pick_list(
                self.sound_files.as_slice(),
                Some(self.sound_settings.file_for(kind).to_owned()),
                move |file| Message::SoundFile(kind, file),
            )
            .text_size(12)
            .padding(8)
            .width(Fill),
            flat_button(
                p,
                text(if playing { "Detener" } else { "Probar" }).size(12),
                if playing {
                    Message::SoundStop
                } else {
                    Message::SoundPreview(kind)
                }
            ),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    }
}
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}
fn kind_icon(kind: EventKind) -> Icon {
    match kind {
        EventKind::Meet | EventKind::Zoom | EventKind::Teams => Icon::Video,
        EventKind::Link => Icon::Link,
        EventKind::Place => Icon::Pin,
        EventKind::Focus => Icon::Focus,
    }
}
fn surface(color: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(color.into()),
        border: border::rounded(radius),
        ..Default::default()
    }
}
fn divider<'a>(p: Palette) -> Element<'a, Message> {
    rule::horizontal(1)
        .style(move |_| rule::Style {
            color: p.line,
            radius: 0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}
fn flat_button<'a>(
    p: Palette,
    content: impl Into<Element<'a, Message>>,
    message: Message,
) -> Element<'a, Message> {
    button(content)
        .on_press(message)
        .padding([3, 1])
        .style(move |_, status| button::Style {
            background: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Some(p.surface.into())
            } else {
                None
            },
            text_color: p.secondary,
            border: border::rounded(7),
            ..Default::default()
        })
        .into()
}
fn icon_button<'a>(
    p: Palette,
    kind: Icon,
    label: &'a str,
    message: Message,
) -> Element<'a, Message> {
    tooltip(
        button(
            container(icon(kind, 14., p.secondary))
                .center_x(25)
                .center_y(27),
        )
        .on_press(message)
        .padding(0)
        .style(move |_, status| button::Style {
            background: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Some(p.surface.into())
            } else {
                None
            },
            text_color: p.secondary,
            border: border::rounded(7),
            ..Default::default()
        }),
        container(text(label).size(11))
            .padding(6)
            .style(move |_| surface(p.surface, 6.)),
        tooltip::Position::Bottom,
    )
    .delay(std::time::Duration::from_millis(500))
    .into()
}
fn action<'a>(p: Palette, label: &'a str, message: Message) -> Element<'a, Message> {
    action_enabled(p, label, Some(message))
}
fn action_enabled<'a>(
    p: Palette,
    label: &'a str,
    message: Option<Message>,
) -> Element<'a, Message> {
    button(container(text(label).size(12).font(medium())).center_x(Fill))
        .on_press_maybe(message)
        .padding([9, 12])
        .style(move |_, status| button::Style {
            background: Some(
                if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    hex(0x35373B)
                } else {
                    p.selected
                }
                .into(),
            ),
            text_color: p.background,
            border: border::rounded(9),
            ..Default::default()
        })
        .into()
}
