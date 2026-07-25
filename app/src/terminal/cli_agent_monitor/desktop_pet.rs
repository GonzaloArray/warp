//! Desktop floating pet — Sumanos PNG + sticky speech-bubble alerts.
//!
//! - Left-drag on the avatar moves the window.
//! - Right-click closes the pet (agents keep running).
//! - Left-click the speech bubble opens the finished session (review path).
//! OS notification center is sent separately by Workspace.

use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::vec2f;
use warpui::assets::asset_cache::AssetSource;
use pathfinder_color::ColorU;
use warpui::elements::{
    Align, Border, CacheOption, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    DispatchEventResult, Element, Empty, EventHandler, Flex, Hoverable, Image, MainAxisAlignment,
    MainAxisSize, MouseStateHandle, Padding, ParentElement, Radius, Shrinkable,
};
use warpui::fonts::Weight;
use warpui::platform::Cursor;
use warpui::platform::{TerminationMode, WindowBounds, WindowStyle};
use warpui::ui_components::components::{UiComponent, UiComponentStyles};
use warpui::windowing::WindowManager;
use warpui::{
    AddWindowOptions, AppContext, Entity, NextNewWindowsHasThisWindowsBoundsUponClose,
    SingletonEntity, TypedActionView, View, ViewContext, ViewHandle, WindowId,
};

use crate::appearance::Appearance;
use crate::terminal::cli_agent_monitor::model::{CliAgentMonitorModel, PetAlert};
use crate::terminal::cli_agent_monitor::pet::{PetMode, PetPosition};
use crate::workspace::{WorkspaceAction, WorkspaceRegistry};

const AVATAR_SIZE: f32 = 88.;
const PET_IDLE_W: f32 = 96.;
const PET_IDLE_H: f32 = 96.;
const PET_ALERT_W: f32 = 340.;
const PET_ALERT_H: f32 = 120.;
const BUBBLE_W: f32 = 220.;
const SUMANOS_AVATAR_PATH: &str = "bundled/png/sumanos-avatar.png";

/// Tracks the global desktop pet window id (at most one).
#[derive(Default)]
pub struct DesktopPetWindowRegistry {
    window_id: Option<WindowId>,
}

impl DesktopPetWindowRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn window_id(&self) -> Option<WindowId> {
        self.window_id
    }

    pub fn set_window_id(&mut self, id: Option<WindowId>) {
        self.window_id = id;
    }
}

impl Entity for DesktopPetWindowRegistry {
    type Event = ();
}

impl SingletonEntity for DesktopPetWindowRegistry {}

#[derive(Debug, Clone)]
pub enum DesktopPetAction {
    /// Open agent monitor panel in the main window (best-effort).
    OpenMonitor,
    /// Hide pet (CLOSED) — does not stop monitoring.
    ClosePet,
    /// User viewed the sticky alert → activate session + clear bubble.
    ActivateAlert { session_id: String },
    /// Hide speech bubble only (does not mark session reviewed).
    DismissAlert,
    Refresh,
}

pub enum DesktopPetEvent {
    ClosePet,
    OpenMonitor,
    Activate { session_id: String },
}

/// Root view of the floating desktop pet window.
pub struct DesktopPetView {
    body_mouse: MouseStateHandle,
    bubble_mouse: MouseStateHandle,
}

impl DesktopPetView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        // Re-render + resize when the monitor model changes (new alert / reviewed).
        ctx.observe(&CliAgentMonitorModel::handle(ctx), |me, _, ctx| {
            me.sync_window_size_for_alert(ctx);
            ctx.notify();
        });
        Self {
            body_mouse: MouseStateHandle::default(),
            bubble_mouse: MouseStateHandle::default(),
        }
    }

    fn sync_window_size_for_alert(&self, ctx: &mut ViewContext<Self>) {
        let has_alert = CliAgentMonitorModel::as_ref(ctx).active_alert().is_some();
        let size = if has_alert {
            vec2f(PET_ALERT_W, PET_ALERT_H)
        } else {
            vec2f(PET_IDLE_W, PET_IDLE_H)
        };
        let window_id = ctx.window_id();
        let Some(current) = ctx.window_bounds(&window_id) else {
            return;
        };
        // Keep top-right corner stable when expanding for the bubble (bubble is to the left).
        let new_origin = if has_alert {
            vec2f(
                current.origin().x() + current.width() - size.x(),
                current.origin().y(),
            )
        } else {
            vec2f(
                current.origin().x() + current.width() - size.x(),
                current.origin().y(),
            )
        };
        let bounds = RectF::new(new_origin, size);
        if (current.width() - size.x()).abs() > 1. || (current.height() - size.y()).abs() > 1. {
            ctx.set_and_cache_window_bounds(window_id, bounds);
        }
    }

    fn activate_session_in_workspace(session_id: &str, ctx: &mut ViewContext<Self>) {
        // Prefer a workspace that still hosts the terminal, otherwise first workspace.
        let terminal_raw = CliAgentMonitorModel::as_ref(ctx)
            .store()
            .get(session_id)
            .and_then(|s| s.terminal_view_id.clone());
        let workspaces = WorkspaceRegistry::as_ref(ctx).all_workspaces(ctx);
        let mut chosen = workspaces.first().map(|(_, ws)| ws.clone());
        if let Some(raw) = terminal_raw.as_deref() {
            if let Ok(id_usize) = raw.parse::<usize>() {
                let entity_id = warpui::EntityId::from_usize(id_usize);
                for (_win, ws) in &workspaces {
                    let has = ws.as_ref(ctx).tab_views().any(|pg| {
                        pg.as_ref(ctx)
                            .find_pane_id_for_terminal_view(entity_id, ctx)
                            .is_some()
                    });
                    if has {
                        chosen = Some(ws.clone());
                        break;
                    }
                }
            }
        }
        if let Some(workspace) = chosen {
            let win = workspace.window_id(ctx);
            ctx.windows().show_window_and_focus_app(win);
            workspace.update(ctx, |ws, ctx| {
                TypedActionView::handle_action(
                    ws,
                    &WorkspaceAction::ActivateCliAgentMonitorSession {
                        session_id: session_id.to_owned(),
                    },
                    ctx,
                );
            });
        }
    }

    fn render_avatar() -> Box<dyn Element> {
        ConstrainedBox::new(
            Image::new(
                AssetSource::Bundled {
                    path: SUMANOS_AVATAR_PATH,
                },
                CacheOption::BySize,
            )
            .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
            .finish(),
        )
        .with_width(AVATAR_SIZE)
        .with_height(AVATAR_SIZE)
        .finish()
    }

    fn render_bubble(
        alert: &PetAlert,
        appearance: &Appearance,
        bubble_mouse: MouseStateHandle,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let surface = theme.surface_2();
        let outline = theme.outline();
        let title_color = theme.main_text_color(surface);
        let body_color = theme.sub_text_color(surface);
        let font = appearance.ui_font_family();
        let ui_builder = appearance.ui_builder().clone();
        let title_str = alert.title.clone();
        let body_str = alert.body.clone();
        let session_id = alert.session_id.clone();

        EventHandler::new(
            Hoverable::new(bubble_mouse, move |state| {
                let title = ui_builder
                    .wrappable_text(title_str.clone(), false)
                    .with_style(UiComponentStyles {
                        font_size: Some(12.),
                        font_weight: Some(Weight::Semibold),
                        font_color: Some(title_color.into()),
                        font_family_id: Some(font),
                        ..Default::default()
                    })
                    .build()
                    .finish();
                let body = ui_builder
                    .wrappable_text(body_str.clone(), false)
                    .with_style(UiComponentStyles {
                        font_size: Some(11.),
                        font_color: Some(body_color.into()),
                        font_family_id: Some(font),
                        ..Default::default()
                    })
                    .build()
                    .finish();
                let hint = ui_builder
                    .wrappable_text("Click: abrir · ✕: ocultar burbuja", false)
                    .with_style(UiComponentStyles {
                        font_size: Some(9.),
                        font_color: Some(body_color.into()),
                        font_family_id: Some(font),
                        ..Default::default()
                    })
                    .build()
                    .finish();
                let dismiss = ui_builder
                    .wrappable_text("✕", false)
                    .with_style(UiComponentStyles {
                        font_size: Some(12.),
                        font_weight: Some(Weight::Bold),
                        font_color: Some(title_color.into()),
                        font_family_id: Some(font),
                        ..Default::default()
                    })
                    .build()
                    .finish();
                let header = Flex::row()
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_child(Shrinkable::new(1., title).finish())
                    .with_child(dismiss)
                    .finish();
                let column = Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_main_axis_size(MainAxisSize::Min)
                    .with_spacing(2.)
                    .with_child(header)
                    .with_child(body)
                    .with_child(hint)
                    .finish();
                let mut bubble = Container::new(column)
                    .with_padding(Padding::default().with_vertical(8.).with_horizontal(10.))
                    .with_background_color(surface.into())
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(12.)))
                    .with_border(Border::all(1.).with_border_color(outline.into()));
                if state.is_hovered() {
                    bubble = bubble.with_background_color(theme.surface_3().into());
                }
                bubble.finish()
            })
            .with_cursor(Cursor::PointingHand)
            .finish(),
        )
        .on_left_mouse_down(move |ctx, _, _| {
            ctx.dispatch_typed_action(DesktopPetAction::ActivateAlert {
                session_id: session_id.clone(),
            });
            DispatchEventResult::StopPropagation
        })
        .on_right_mouse_down(|ctx, _, _| {
            // Right-click on bubble: dismiss bubble only (keep pet + session).
            ctx.dispatch_typed_action(DesktopPetAction::DismissAlert);
            DispatchEventResult::StopPropagation
        })
        .finish()
    }
}

impl Default for DesktopPetView {
    fn default() -> Self {
        Self {
            body_mouse: MouseStateHandle::default(),
            bubble_mouse: MouseStateHandle::default(),
        }
    }
}

impl Entity for DesktopPetView {
    type Event = DesktopPetEvent;
}

impl TypedActionView for DesktopPetView {
    type Action = DesktopPetAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            DesktopPetAction::ClosePet => {
                CliAgentMonitorModel::handle(ctx).update(ctx, |m, ctx| m.close_pet(ctx));
                DesktopPetWindowRegistry::handle(ctx).update(ctx, |reg, _| {
                    reg.set_window_id(None);
                });
                ctx.emit(DesktopPetEvent::ClosePet);
                ctx.close_window();
            }
            DesktopPetAction::OpenMonitor => {
                ctx.emit(DesktopPetEvent::OpenMonitor);
            }
            DesktopPetAction::ActivateAlert { session_id } => {
                let id = session_id.clone();
                Self::activate_session_in_workspace(&id, ctx);
                ctx.emit(DesktopPetEvent::Activate {
                    session_id: id.clone(),
                });
                self.sync_window_size_for_alert(ctx);
                ctx.notify();
            }
            DesktopPetAction::DismissAlert => {
                CliAgentMonitorModel::handle(ctx).update(ctx, |m, ctx| {
                    m.dismiss_active_alert(ctx);
                });
                self.sync_window_size_for_alert(ctx);
                ctx.notify();
            }
            DesktopPetAction::Refresh => ctx.notify(),
        }
    }
}

impl View for DesktopPetView {
    fn ui_name() -> &'static str {
        "DesktopPetView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let monitor = CliAgentMonitorModel::as_ref(app);
        let alert = monitor.active_alert().cloned();
        let pending = monitor.store().pending_review_count();
        let body_mouse = self.body_mouse.clone();
        let bubble_mouse = self.bubble_mouse.clone();

        // Always-visible ✕ so a stuck pet can always be closed (left-click).
        let close_mouse = MouseStateHandle::default();
        let ui_builder = appearance.ui_builder().clone();
        let font = appearance.ui_font_family();
        let theme = appearance.theme();
        let fg = theme.main_text_color(theme.surface_2());
        let hover_bg = theme.surface_3();
        let close_btn = EventHandler::new(
            Hoverable::new(close_mouse, move |state| {
                let label = ui_builder
                    .wrappable_text("✕", false)
                    .with_style(UiComponentStyles {
                        font_size: Some(12.),
                        font_weight: Some(Weight::Bold),
                        font_color: Some(fg.into()),
                        font_family_id: Some(font),
                        ..Default::default()
                    })
                    .build()
                    .finish();
                let mut c = Container::new(Align::new(label).finish())
                    .with_padding(Padding::default().with_vertical(2.).with_horizontal(6.))
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)));
                if state.is_hovered() {
                    c = c.with_background_color(hover_bg.into());
                }
                c.finish()
            })
            .with_cursor(Cursor::PointingHand)
            .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(DesktopPetAction::ClosePet);
            DispatchEventResult::StopPropagation
        })
        .finish();

        // Avatar: unhandled left-clicks → native window drag. Right-click closes.
        let avatar = EventHandler::new(
            Hoverable::new(body_mouse, move |_| Self::render_avatar())
                .with_cursor(Cursor::Arrow)
                .finish(),
        )
        .on_right_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(DesktopPetAction::ClosePet);
            DispatchEventResult::StopPropagation
        })
        .finish();

        // Pending-review pulse (small red badge) even without a text bubble.
        let avatar_stack = if pending > 0 {
            let badge = ConstrainedBox::new(
                Container::new(Empty::new().finish())
                    .with_background_color(ColorU::new(255, 69, 58, 255).into())
                    .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                    .finish(),
            )
            .with_width(14.)
            .with_height(14.)
            .finish();
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::End)
                .with_main_axis_size(MainAxisSize::Min)
                .with_child(close_btn)
                .with_child(badge)
                .with_child(avatar)
                .finish()
        } else {
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::End)
                .with_main_axis_size(MainAxisSize::Min)
                .with_child(close_btn)
                .with_child(avatar)
                .finish()
        };
        let avatar_with_badge = avatar_stack;

        if let Some(alert) = alert {
            let bubble = ConstrainedBox::new(Self::render_bubble(
                &alert,
                appearance,
                bubble_mouse,
            ))
            .with_width(BUBBLE_W)
            .finish();

            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_main_axis_alignment(MainAxisAlignment::End)
                .with_main_axis_size(MainAxisSize::Max)
                .with_spacing(8.)
                .with_child(Shrinkable::new(1., bubble).finish())
                .with_child(avatar_with_badge)
                .finish()
        } else {
            Align::new(avatar_with_badge).finish()
        }
    }
}

fn default_pet_bounds(ctx: &AppContext, prefs_pos: Option<PetPosition>) -> WindowBounds {
    let size = vec2f(PET_IDLE_W, PET_IDLE_H);
    if let Some(PetPosition { x, y }) = prefs_pos {
        return WindowBounds::ExactPosition(RectF::new(vec2f(x, y), size));
    }
    let display = WindowManager::as_ref(ctx).active_display_bounds();
    let margin = 24.;
    let origin = vec2f(
        display.origin().x() + display.width() - size.x() - margin,
        display.origin().y() + margin + 40.,
    );
    WindowBounds::ExactPosition(RectF::new(origin, size))
}

/// Open the desktop pet window if not already open. Returns window id.
pub fn ensure_desktop_pet_window(ctx: &mut AppContext) -> Option<WindowId> {
    if !CliAgentMonitorModel::as_ref(ctx).may_show_floating_pet() {
        return None;
    }
    if let Some(id) = DesktopPetWindowRegistry::as_ref(ctx).window_id() {
        let still_open = ctx.window_ids().any(|w| w == id);
        if still_open {
            WindowManager::as_ref(ctx).show_window_and_focus_app(id);
            return Some(id);
        }
        DesktopPetWindowRegistry::handle(ctx).update(ctx, |reg, _| {
            reg.set_window_id(None);
        });
    }

    let pos = CliAgentMonitorModel::as_ref(ctx)
        .pet()
        .prefs()
        .position;
    let has_alert = CliAgentMonitorModel::as_ref(ctx).active_alert().is_some();
    let mut bounds = default_pet_bounds(ctx, pos);
    if has_alert {
        if let WindowBounds::ExactPosition(rect) = bounds {
            let size = vec2f(PET_ALERT_W, PET_ALERT_H);
            let origin = vec2f(
                rect.origin().x() + rect.width() - size.x(),
                rect.origin().y(),
            );
            bounds = WindowBounds::ExactPosition(RectF::new(origin, size));
        }
    }

    let options = AddWindowOptions {
        window_style: WindowStyle::FloatingMovable,
        window_bounds: bounds,
        title: Some("Sumanos Pet".to_owned()),
        background_blur_radius_pixels: Some(0),
        background_blur_texture: false,
        anchor_new_windows_from_closed_position: NextNewWindowsHasThisWindowsBoundsUponClose::No,
        window_instance: Some("dev.warp.WarpOss.SumanosPet".to_owned()),
        ..Default::default()
    };

    let (window_id, _handle): (WindowId, ViewHandle<DesktopPetView>) =
        ctx.add_window(options, |ctx| DesktopPetView::new(ctx));

    DesktopPetWindowRegistry::handle(ctx).update(ctx, |reg, _| {
        reg.set_window_id(Some(window_id));
    });

    log::info!("Opened Sumanos desktop pet window {window_id:?}");
    Some(window_id)
}

/// Close the desktop pet window if open (does not change agent sessions).
pub fn close_desktop_pet_window(ctx: &mut AppContext) {
    let id = DesktopPetWindowRegistry::as_ref(ctx).window_id();
    if let Some(window_id) = id {
        DesktopPetWindowRegistry::handle(ctx).update(ctx, |reg, _| {
            reg.set_window_id(None);
        });
        WindowManager::as_ref(ctx).close_window(window_id, TerminationMode::Cancellable);
        log::info!("Closed Sumanos desktop pet window {window_id:?}");
    }
}

/// Sync window open/closed state with pet mode.
pub fn sync_desktop_pet_window_to_mode(ctx: &mut AppContext) {
    let mode = CliAgentMonitorModel::as_ref(ctx).pet_mode();
    match mode {
        PetMode::Visible => {
            let _ = ensure_desktop_pet_window(ctx);
        }
        PetMode::Minimized | PetMode::Closed => {
            close_desktop_pet_window(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PET_ALERT_W, PET_IDLE_W, SUMANOS_AVATAR_PATH};

    #[test]
    fn desktop_pet_uses_sumanos_avatar_and_grows_for_alerts() {
        assert_eq!(SUMANOS_AVATAR_PATH, "bundled/png/sumanos-avatar.png");
        assert!(PET_IDLE_W <= 120.);
        assert!(PET_ALERT_W > PET_IDLE_W);
    }
}
