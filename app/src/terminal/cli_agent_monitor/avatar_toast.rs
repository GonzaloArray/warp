//! In-app toast: Sumanos mascot avatar + message bubble under the tab bar.
//!
//! Shown when a CLI agent finishes (alongside the desktop notification).

use std::time::Duration;

use pathfinder_geometry::vector::vec2f;
use warpui::r#async::{SpawnedFutureHandle, Timer};
use warpui::assets::asset_cache::AssetSource;
use warpui::elements::{
    Border, CacheOption, ChildAnchor, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    DispatchEventResult, Element, Empty, EventHandler, Flex, Hoverable, Image, MainAxisSize,
    MouseStateHandle, OffsetPositioning, Padding, ParentElement, PositionedElementAnchor,
    PositionedElementOffsetBounds, Radius, Shrinkable,
};
use warpui::fonts::Weight;
use warpui::platform::Cursor;
use warpui::ui_components::components::{UiComponent, UiComponentStyles};
use warpui::{AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext};

use crate::appearance::Appearance;

const AVATAR_SIZE: f32 = 40.;
const TOAST_WIDTH: f32 = 340.;
const TOAST_DURATION_SECS: u64 = 8;
const SUMANOS_AVATAR_PATH: &str = "bundled/png/sumanos-avatar.png";

#[derive(Clone, Debug)]
pub struct AvatarToastMessage {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub session_id: String,
}

struct ToastEntry {
    message: AvatarToastMessage,
    mouse: MouseStateHandle,
    abort: Option<SpawnedFutureHandle>,
}

pub enum CliAgentAvatarToastEvent {
    Activate { session_id: String },
    /// User closed the pet (hide floating UI; monitor keeps running).
    ClosePet,
}

#[derive(Debug, Clone)]
pub enum CliAgentAvatarToastAction {
    Dismiss(u64),
    Click(u64),
    /// Hide pet permanently (preference) — tooltip “Cerrar pet”.
    ClosePet,
}

/// Stack of Sumanos-avatar message toasts (top of window, under tab bar).
pub struct CliAgentAvatarToastStack {
    next_id: u64,
    toasts: Vec<ToastEntry>,
    /// When false, floating toasts are not shown (pet CLOSED / MINIMIZED).
    floating_enabled: bool,
    close_button_mouse: MouseStateHandle,
}

impl CliAgentAvatarToastStack {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            toasts: Vec::new(),
            floating_enabled: true,
            close_button_mouse: MouseStateHandle::default(),
        }
    }

    pub fn set_floating_enabled(&mut self, enabled: bool, ctx: &mut ViewContext<Self>) {
        self.floating_enabled = enabled;
        if !enabled {
            self.dismiss_all_visual(ctx);
        }
        ctx.notify();
    }

    pub fn floating_enabled(&self) -> bool {
        self.floating_enabled
    }

    /// Stop animations / clear visible toasts without touching monitor data.
    fn dismiss_all_visual(&mut self, ctx: &mut ViewContext<Self>) {
        for entry in self.toasts.drain(..) {
            if let Some(h) = entry.abort {
                h.abort();
            }
        }
        ctx.notify();
    }

    /// Public entry for Workspace to force-clear stuck toasts.
    pub fn dismiss_all_visual_public(&mut self, ctx: &mut ViewContext<Self>) {
        self.dismiss_all_visual(ctx);
    }

    pub fn push_message(
        &mut self,
        title: String,
        body: String,
        session_id: String,
        ctx: &mut ViewContext<Self>,
    ) {
        // Pet CLOSED / MINIMIZED: queue is not shown; desktop notify still sent by Workspace.
        if !self.floating_enabled {
            return;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let message = AvatarToastMessage {
            id,
            title,
            body,
            session_id,
        };
        let mut entry = ToastEntry {
            message,
            mouse: MouseStateHandle::default(),
            abort: None,
        };
        Self::arm_dismiss_timer(&mut entry, ctx);
        self.toasts.push(entry);
        while self.toasts.len() > 3 {
            if let Some(old) = self.toasts.first_mut()
                && let Some(h) = old.abort.take()
            {
                h.abort();
            }
            self.toasts.remove(0);
        }
        ctx.notify();
    }

    fn arm_dismiss_timer(entry: &mut ToastEntry, ctx: &mut ViewContext<Self>) {
        let id = entry.message.id;
        if let Some(h) = entry.abort.take() {
            h.abort();
        }
        entry.abort = Some(ctx.spawn_abortable(
            Timer::after(Duration::from_secs(TOAST_DURATION_SECS)),
            move |me, _, ctx| me.dismiss(id, ctx),
            |_, _| {},
        ));
    }

    fn dismiss(&mut self, id: u64, ctx: &mut ViewContext<Self>) {
        if let Some(idx) = self.toasts.iter().position(|t| t.message.id == id) {
            let entry = self.toasts.remove(idx);
            if let Some(h) = entry.abort {
                h.abort();
            }
            ctx.notify();
        }
    }

    fn render_toast(entry: &ToastEntry, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let surface = theme.surface_2();
        let title_color = theme.main_text_color(surface);
        let body_color = theme.sub_text_color(surface);
        let id = entry.message.id;
        let mouse = entry.mouse.clone();
        let title_str = entry.message.title.clone();
        let body_str = entry.message.body.clone();
        let font = appearance.ui_font_family();
        let ui_builder = appearance.ui_builder().clone();
        let outline = theme.outline();
        let surface_3 = theme.surface_3();

        EventHandler::new(
            Hoverable::new(mouse, move |_| {
                let title = ui_builder
                    .wrappable_text(title_str.clone(), false)
                    .with_style(UiComponentStyles {
                        font_size: Some(13.),
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
                        font_size: Some(12.),
                        font_color: Some(body_color.into()),
                        font_family_id: Some(font),
                        ..Default::default()
                    })
                    .build()
                    .finish();
                let bubble_text = Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_main_axis_size(MainAxisSize::Min)
                    .with_spacing(2.)
                    .with_child(title)
                    .with_child(body)
                    .finish();
                let bubble = Container::new(bubble_text)
                    .with_padding(Padding::default().with_vertical(8.).with_horizontal(12.))
                    .with_background_color(surface.into())
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(12.)))
                    .with_border(Border::all(1.).with_border_color(outline.into()))
                    .finish();
                let avatar = ConstrainedBox::new(
                    Container::new(
                        Image::new(
                            AssetSource::Bundled {
                                path: SUMANOS_AVATAR_PATH,
                            },
                            CacheOption::BySize,
                        )
                        .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                        .finish(),
                    )
                    .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                    .with_border(Border::all(1.).with_border_color(outline.into()))
                    .with_background_color(surface_3.into())
                    .finish(),
                )
                .with_width(AVATAR_SIZE)
                .with_height(AVATAR_SIZE)
                .finish();
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_main_axis_size(MainAxisSize::Min)
                    .with_spacing(10.)
                    .with_child(avatar)
                    .with_child(Shrinkable::new(1., bubble).finish())
                    .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .finish(),
        )
        .on_left_mouse_down(move |ctx, _, _| {
            ctx.dispatch_typed_action(CliAgentAvatarToastAction::Click(id));
            DispatchEventResult::StopPropagation
        })
        .finish()
    }
}

impl Default for CliAgentAvatarToastStack {
    fn default() -> Self {
        Self::new()
    }
}

impl Entity for CliAgentAvatarToastStack {
    type Event = CliAgentAvatarToastEvent;
}

impl TypedActionView for CliAgentAvatarToastStack {
    type Action = CliAgentAvatarToastAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            CliAgentAvatarToastAction::Dismiss(id) => self.dismiss(*id, ctx),
            CliAgentAvatarToastAction::Click(id) => {
                if let Some(entry) = self.toasts.iter().find(|t| t.message.id == *id) {
                    let session_id = entry.message.session_id.clone();
                    ctx.emit(CliAgentAvatarToastEvent::Activate { session_id });
                }
                self.dismiss(*id, ctx);
            }
            CliAgentAvatarToastAction::ClosePet => {
                self.dismiss_all_visual(ctx);
                self.floating_enabled = false;
                ctx.emit(CliAgentAvatarToastEvent::ClosePet);
                ctx.notify();
            }
        }
    }
}

impl View for CliAgentAvatarToastStack {
    fn ui_name() -> &'static str {
        "CliAgentAvatarToastStack"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        if !self.floating_enabled || self.toasts.is_empty() {
            return Empty::new().finish();
        }

        let mut column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::End)
            .with_main_axis_size(MainAxisSize::Min)
            .with_spacing(8.);

        // Close-pet control above the stack (accessible, reversible).
        column.add_child(self.render_close_pet_button(appearance));

        for entry in self.toasts.iter().rev() {
            column.add_child(
                ConstrainedBox::new(Self::render_toast(entry, appearance))
                    .with_width(TOAST_WIDTH)
                    .finish(),
            );
        }

        column.finish()
    }
}

impl CliAgentAvatarToastStack {
    /// Tooltip “Cerrar pet”; large hit target; keyboard via typed action.
    fn render_close_pet_button(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let surface = theme.surface_2();
        let text_color = theme.main_text_color(surface);
        let mouse = self.close_button_mouse.clone();
        let ui_builder = appearance.ui_builder().clone();
        let font = appearance.ui_font_family();

        EventHandler::new(
            Hoverable::new(mouse, move |state| {
                let label = ui_builder
                    .wrappable_text("✕  Cerrar pet".to_string(), false)
                    .with_style(UiComponentStyles {
                        font_size: Some(11.),
                        font_weight: Some(Weight::Semibold),
                        font_color: Some(text_color.into()),
                        font_family_id: Some(font),
                        ..Default::default()
                    })
                    .build()
                    .finish();
                let mut c = Container::new(label)
                    .with_padding(Padding::default().with_vertical(6.).with_horizontal(10.))
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
                    .with_border(Border::all(1.).with_border_color(theme.outline().into()));
                if state.is_hovered() {
                    c = c.with_background_color(theme.surface_3().into());
                } else {
                    c = c.with_background_color(surface.into());
                }
                c.finish()
            })
            .with_cursor(Cursor::PointingHand)
            .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            // Accessible label is on the button itself (“✕  Cerrar pet”).
            ctx.dispatch_typed_action(CliAgentAvatarToastAction::ClosePet);
            DispatchEventResult::StopPropagation
        })
        .finish()
    }
}

/// Place under the tab bar, top-right (message-style).
pub fn avatar_toast_positioning(tab_bar_position_id: &str) -> OffsetPositioning {
    OffsetPositioning::offset_from_save_position_element(
        tab_bar_position_id,
        vec2f(-12., 8.),
        PositionedElementOffsetBounds::WindowByPosition,
        PositionedElementAnchor::BottomRight,
        ChildAnchor::TopRight,
    )
}

#[cfg(test)]
mod tests {
    use super::SUMANOS_AVATAR_PATH;

    #[test]
    fn sumanos_avatar_asset_path_is_bundled_png() {
        assert_eq!(SUMANOS_AVATAR_PATH, "bundled/png/sumanos-avatar.png");
        assert!(
            std::path::Path::new("app/assets/bundled/png/sumanos-avatar.png").exists()
                || std::path::Path::new("assets/bundled/png/sumanos-avatar.png").exists()
                || std::path::Path::new("bundled/png/sumanos-avatar.png").exists()
        );
    }
}
