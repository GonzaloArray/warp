//! WarpUI single-column panel for the CLI agent monitor.

use pathfinder_color::ColorU;
use warpui::elements::{
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, DispatchEventResult, Element,
    Empty, EventHandler, Flex, Hoverable, MainAxisSize, MouseStateHandle, Padding, ParentElement,
    Radius, Shrinkable,
};
use warpui::fonts::Weight;
use warpui::ui_components::components::{UiComponent, UiComponentStyles};
use warpui::{AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext};

use super::model::CliAgentMonitorModel;
use super::pet::PetMode;
use super::session::MonitorSession;
use super::ui::PanelRow;

const PANEL_WIDTH: f32 = 320.;
const PANEL_MAX_HEIGHT: f32 = 420.;

pub enum CliAgentMonitorPanelEvent {
    ActivateSession { session_id: String },
    MarkReviewed { session_id: String },
    RemoveSession { session_id: String },
    ClearAll,
    ShowPet,
    HidePet,
    ToggleDesktopAlerts,
    ToggleWhatsAppAlerts,
    Dismissed,
}

#[derive(Debug, Clone)]
pub enum CliAgentMonitorPanelAction {
    ActivateSession { session_id: String },
    MarkReviewed { session_id: String },
    RemoveSession { session_id: String },
    ClearAll,
    ShowPet,
    HidePet,
    ToggleDesktopAlerts,
    ToggleWhatsAppAlerts,
    Dismiss,
}

pub struct CliAgentMonitorPanelView {
    row_mouse_states: Vec<MouseStateHandle>,
    mark_mouse_states: Vec<MouseStateHandle>,
}

impl CliAgentMonitorPanelView {
    pub fn new() -> Self {
        Self {
            row_mouse_states: Vec::new(),
            mark_mouse_states: Vec::new(),
        }
    }

    pub fn prepare_for_rows(&mut self, count: usize) {
        self.row_mouse_states
            .resize_with(count, MouseStateHandle::default);
        self.mark_mouse_states
            .resize_with(count, MouseStateHandle::default);
    }

    fn color_for_token(token: &str) -> ColorU {
        match token {
            "green" => ColorU::new(52, 199, 89, 255),
            "yellow" => ColorU::new(255, 204, 0, 255),
            "red" => ColorU::new(255, 69, 58, 255),
            "neutral_animated" => ColorU::new(100, 149, 237, 255),
            "gray" => ColorU::new(142, 142, 147, 255),
            _ => ColorU::new(142, 142, 147, 255),
        }
    }

    fn render_row(
        &self,
        index: usize,
        row: &PanelRow,
        appearance: &crate::appearance::Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let surface = theme.surface_2();
        let text_color = theme.main_text_color(surface);
        let sub_color = theme.sub_text_color(surface);
        let dot_color = Self::color_for_token(row.color_token);

        let session_id = row.session_id.clone();
        let _mouse = self
            .row_mouse_states
            .get(index)
            .cloned()
            .unwrap_or_default();

        let title = format!("{} · {}", row.agent, row.project);
        let subtitle = format!(
            "{} · {}{}",
            row.state_label,
            row.elapsed_label,
            if row.review_pending {
                " · pendiente de revisión"
            } else {
                ""
            }
        );

        let title_el = appearance
            .ui_builder()
            .wrappable_text(title, false)
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                font_weight: Some(Weight::Semibold),
                font_color: Some(text_color.into()),
                font_family_id: Some(appearance.ui_font_family()),
                ..Default::default()
            })
            .build()
            .finish();

        let subtitle_el = appearance
            .ui_builder()
            .wrappable_text(subtitle, false)
            .with_style(UiComponentStyles {
                font_size: Some(11.),
                font_color: Some(sub_color.into()),
                font_family_id: Some(appearance.ui_font_family()),
                ..Default::default()
            })
            .build()
            .finish();

        let mut texts = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_main_axis_size(MainAxisSize::Min)
            .with_child(title_el)
            .with_child(subtitle_el);

        if row.show_mark_reviewed {
            let mark_id = row.session_id.clone();
            let mark_label = appearance
                .ui_builder()
                .wrappable_text("Marcar como revisado".to_string(), false)
                .with_style(UiComponentStyles {
                    font_size: Some(11.),
                    font_weight: Some(Weight::Semibold),
                    font_color: Some(ColorU::new(10, 132, 255, 255).into()),
                    font_family_id: Some(appearance.ui_font_family()),
                    ..Default::default()
                })
                .build()
                .finish();
            let mark_btn = EventHandler::new(
                Container::new(mark_label)
                    .with_padding(Padding::default().with_vertical(2.))
                    .finish(),
            )
            .on_left_mouse_down(move |ctx, _, _| {
                ctx.dispatch_typed_action(CliAgentMonitorPanelAction::MarkReviewed {
                    session_id: mark_id.clone(),
                });
                DispatchEventResult::StopPropagation
            })
            .finish();
            texts.add_child(mark_btn);
        }

        // Always allow removing a stuck row (zombies cannot be "reviewed" otherwise).
        {
            let remove_id = row.session_id.clone();
            let remove_label = appearance
                .ui_builder()
                .wrappable_text("Quitar".to_string(), false)
                .with_style(UiComponentStyles {
                    font_size: Some(11.),
                    font_weight: Some(Weight::Semibold),
                    font_color: Some(ColorU::new(255, 69, 58, 255).into()),
                    font_family_id: Some(appearance.ui_font_family()),
                    ..Default::default()
                })
                .build()
                .finish();
            let remove_btn = EventHandler::new(
                Container::new(remove_label)
                    .with_padding(Padding::default().with_vertical(2.))
                    .finish(),
            )
            .on_left_mouse_down(move |ctx, _, _| {
                ctx.dispatch_typed_action(CliAgentMonitorPanelAction::RemoveSession {
                    session_id: remove_id.clone(),
                });
                DispatchEventResult::StopPropagation
            })
            .finish();
            texts.add_child(remove_btn);
        }

        let status_dot = ConstrainedBox::new(
            Container::new(Empty::new().finish())
                .with_background_color(dot_color)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
                .finish(),
        )
        .with_width(8.)
        .with_height(8.)
        .finish();

        let texts_el = texts.finish();
        let row_body = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_main_axis_size(MainAxisSize::Max)
            .with_spacing(8.)
            .with_child(status_dot)
            .with_child(Shrinkable::new(1., texts_el).finish())
            .finish();

        // No hover-rebuild clone (Element is not Clone): fixed padding row + click.
        EventHandler::new(
            Container::new(row_body)
                .with_padding(Padding::default().with_vertical(6.).with_horizontal(10.))
                .finish(),
        )
        .on_left_mouse_down(move |ctx, _, _| {
            ctx.dispatch_typed_action(CliAgentMonitorPanelAction::ActivateSession {
                session_id: session_id.clone(),
            });
            DispatchEventResult::StopPropagation
        })
        .finish()
    }
}

impl Default for CliAgentMonitorPanelView {
    fn default() -> Self {
        Self::new()
    }
}

impl Entity for CliAgentMonitorPanelView {
    type Event = CliAgentMonitorPanelEvent;
}

impl TypedActionView for CliAgentMonitorPanelView {
    type Action = CliAgentMonitorPanelAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            CliAgentMonitorPanelAction::ActivateSession { session_id } => {
                ctx.emit(CliAgentMonitorPanelEvent::ActivateSession {
                    session_id: session_id.clone(),
                });
            }
            CliAgentMonitorPanelAction::MarkReviewed { session_id } => {
                ctx.emit(CliAgentMonitorPanelEvent::MarkReviewed {
                    session_id: session_id.clone(),
                });
            }
            CliAgentMonitorPanelAction::RemoveSession { session_id } => {
                ctx.emit(CliAgentMonitorPanelEvent::RemoveSession {
                    session_id: session_id.clone(),
                });
            }
            CliAgentMonitorPanelAction::ClearAll => {
                ctx.emit(CliAgentMonitorPanelEvent::ClearAll);
            }
            CliAgentMonitorPanelAction::ShowPet => {
                ctx.emit(CliAgentMonitorPanelEvent::ShowPet);
            }
            CliAgentMonitorPanelAction::HidePet => {
                ctx.emit(CliAgentMonitorPanelEvent::HidePet);
            }
            CliAgentMonitorPanelAction::ToggleDesktopAlerts => {
                ctx.emit(CliAgentMonitorPanelEvent::ToggleDesktopAlerts);
            }
            CliAgentMonitorPanelAction::ToggleWhatsAppAlerts => {
                ctx.emit(CliAgentMonitorPanelEvent::ToggleWhatsAppAlerts);
            }
            CliAgentMonitorPanelAction::Dismiss => {
                ctx.emit(CliAgentMonitorPanelEvent::Dismissed);
            }
        }
    }
}

impl View for CliAgentMonitorPanelView {
    fn ui_name() -> &'static str {
        "CliAgentMonitorPanelView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = crate::appearance::Appearance::as_ref(app);
        let theme = appearance.theme();
        let surface = theme.surface_2();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let monitor = CliAgentMonitorModel::as_ref(app);
        let rows: Vec<PanelRow> = monitor
            .panel_rows(now_ms, |session: &MonitorSession| {
                session.terminal_view_id.is_some()
            })
            .into_iter()
            .collect();

        let indicator = monitor.indicator_text();
        let header_title = appearance
            .ui_builder()
            .wrappable_text(format!("Agentes · {indicator}"), false)
            .with_style(UiComponentStyles {
                font_size: Some(13.),
                font_weight: Some(Weight::Semibold),
                font_color: Some(theme.main_text_color(surface).into()),
                font_family_id: Some(appearance.ui_font_family()),
                ..Default::default()
            })
            .build()
            .finish();
        let clear_label = appearance
            .ui_builder()
            .wrappable_text("Limpiar todos".to_string(), false)
            .with_style(UiComponentStyles {
                font_size: Some(11.),
                font_weight: Some(Weight::Semibold),
                font_color: Some(ColorU::new(255, 69, 58, 255).into()),
                font_family_id: Some(appearance.ui_font_family()),
                ..Default::default()
            })
            .build()
            .finish();
        let clear_btn = EventHandler::new(
            Container::new(clear_label)
                .with_padding(Padding::default().with_vertical(2.).with_horizontal(4.))
                .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(CliAgentMonitorPanelAction::ClearAll);
            DispatchEventResult::StopPropagation
        })
        .finish();
        let header = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(Shrinkable::new(1., header_title).finish())
            .with_child(clear_btn)
            .finish();

        let mut column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Min)
            .with_child(
                Container::new(header)
                    .with_padding(Padding::default().with_vertical(8.).with_horizontal(12.))
                    .finish(),
            );

        if rows.is_empty() {
            let empty = appearance
                .ui_builder()
                .wrappable_text("No hay agentes CLI activos".to_string(), false)
                .with_style(UiComponentStyles {
                    font_size: Some(12.),
                    font_color: Some(theme.sub_text_color(surface).into()),
                    font_family_id: Some(appearance.ui_font_family()),
                    ..Default::default()
                })
                .build()
                .finish();
            column.add_child(
                Container::new(empty)
                    .with_padding(Padding::default().with_vertical(12.).with_horizontal(12.))
                    .finish(),
            );
        } else {
            for (i, row) in rows.iter().enumerate() {
                column.add_child(self.render_row(i, row, appearance));
            }
        }

        // Pet show/hide — always available so the companion is discoverable.
        let pet_visible = matches!(monitor.pet_mode(), PetMode::Visible);
        let pet_label = if pet_visible {
            "Ocultar pet Sumanos"
        } else {
            "Mostrar pet Sumanos"
        };
        let pet_text = appearance
            .ui_builder()
            .wrappable_text(pet_label.to_string(), false)
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                font_weight: Some(Weight::Semibold),
                font_color: Some(ColorU::new(10, 132, 255, 255).into()),
                font_family_id: Some(appearance.ui_font_family()),
                ..Default::default()
            })
            .build()
            .finish();
        let pet_btn = EventHandler::new(
            Container::new(pet_text)
                .with_padding(Padding::default().with_vertical(8.).with_horizontal(12.))
                .finish(),
        )
        .on_left_mouse_down(move |ctx, _, _| {
            if pet_visible {
                ctx.dispatch_typed_action(CliAgentMonitorPanelAction::HidePet);
            } else {
                ctx.dispatch_typed_action(CliAgentMonitorPanelAction::ShowPet);
            }
            DispatchEventResult::StopPropagation
        })
        .finish();
        column.add_child(pet_btn);

        // Alert channels
        let alerts = monitor.alerts();
        let desk_label = if alerts.desktop_enabled {
            "✓ Alerta escritorio (macOS)"
        } else {
            "○ Alerta escritorio (macOS)"
        };
        let desk_text = appearance
            .ui_builder()
            .wrappable_text(desk_label.to_string(), false)
            .with_style(UiComponentStyles {
                font_size: Some(11.),
                font_color: Some(theme.sub_text_color(surface).into()),
                font_family_id: Some(appearance.ui_font_family()),
                ..Default::default()
            })
            .build()
            .finish();
        column.add_child(
            EventHandler::new(
                Container::new(desk_text)
                    .with_padding(Padding::default().with_vertical(4.).with_horizontal(12.))
                    .finish(),
            )
            .on_left_mouse_down(|ctx, _, _| {
                ctx.dispatch_typed_action(CliAgentMonitorPanelAction::ToggleDesktopAlerts);
                DispatchEventResult::StopPropagation
            })
            .finish(),
        );

        let wa_label = if alerts.whatsapp_ready() {
            "✓ WhatsApp (listo)"
        } else if alerts.whatsapp_enabled {
            "⚠ WhatsApp on (falta phone/key)"
        } else {
            "○ WhatsApp off — click para activar"
        };
        let wa_text = appearance
            .ui_builder()
            .wrappable_text(wa_label.to_string(), false)
            .with_style(UiComponentStyles {
                font_size: Some(11.),
                font_color: Some(theme.sub_text_color(surface).into()),
                font_family_id: Some(appearance.ui_font_family()),
                ..Default::default()
            })
            .build()
            .finish();
        column.add_child(
            EventHandler::new(
                Container::new(wa_text)
                    .with_padding(Padding::default().with_vertical(4.).with_horizontal(12.))
                    .finish(),
            )
            .on_left_mouse_down(|ctx, _, _| {
                ctx.dispatch_typed_action(CliAgentMonitorPanelAction::ToggleWhatsAppAlerts);
                DispatchEventResult::StopPropagation
            })
            .finish(),
        );

        if alerts.whatsapp_enabled && !alerts.whatsapp_configured() {
            let help = appearance
                .ui_builder()
                .wrappable_text(
                    "Config: ~/.warp-oss/cli_agent_monitor/alerts.json\n\
                     o env WARP_WHATSAPP_PHONE + WARP_WHATSAPP_API_KEY\n\
                     (CallMeBot gratis)",
                    false,
                )
                .with_style(UiComponentStyles {
                    font_size: Some(10.),
                    font_color: Some(ColorU::new(255, 159, 10, 255).into()),
                    font_family_id: Some(appearance.ui_font_family()),
                    ..Default::default()
                })
                .build()
                .finish();
            column.add_child(
                Container::new(help)
                    .with_padding(Padding::default().with_vertical(4.).with_horizontal(12.))
                    .finish(),
            );
        }

        ConstrainedBox::new(
            Container::new(column.finish())
                .with_background_color(surface.into())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
                .with_border(
                    warpui::elements::Border::all(1.)
                        .with_border_color(theme.outline().into()),
                )
                .finish(),
        )
        .with_width(PANEL_WIDTH)
        .with_max_height(PANEL_MAX_HEIGHT)
        .finish()
    }
}
