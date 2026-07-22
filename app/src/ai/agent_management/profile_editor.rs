//! Small editor for an agent's local display identity (name, avatar, color).

use crate::ai::agent_management::profiles::{
    AgentProfile, AgentProfileEditor, AvatarKind, Palette,
};
use crate::appearance::Appearance;
use crate::editor::Event as EditorEvent;
use crate::editor::{EditorView, SingleLineEditorOptions, TextOptions};
use crate::ui_components::avatar::{Avatar, AvatarContent};
use crate::view_components::action_button::{
    ActionButton, ButtonSize, NakedTheme, PrimaryTheme, SecondaryTheme,
};
use warp_core::ui::icons::Icon as CoreIcon;
use warpui::elements::{
    ChildView, Container, CornerRadius, CrossAxisAlignment, Flex, MainAxisSize, ParentElement,
    Radius, Text,
};
use warpui::fonts::Weight;
use warpui::ui_components::components::{UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Element, Entity, FocusContext, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

const PREVIEW_AVATAR_SIZE: f32 = 36.;

#[derive(Clone, Debug)]
pub enum AgentProfileEditorAction {
    Save,
    Cancel,
    SetIcon(AvatarKind),
    SetPalette(Palette),
    /// Open system file picker for PNG/JPEG avatar.
    PickAvatarPng,
    /// Clear custom PNG and fall back to built-in icon.
    ClearAvatarPng,
    /// Applied after a successful file pick + import.
    SetAvatarPngPath(String),
}

#[derive(Clone, Debug)]
pub enum AgentProfileEditorEvent {
    Saved(AgentProfile),
    Cancelled,
}

pub struct AgentProfileEditorView {
    editor: ViewHandle<EditorView>,
    draft: AgentProfileEditor,
    save_button: ViewHandle<ActionButton>,
    cancel_button: ViewHandle<ActionButton>,
    pick_png_button: ViewHandle<ActionButton>,
    clear_png_button: ViewHandle<ActionButton>,
    icon_buttons: Vec<(AvatarKind, ViewHandle<ActionButton>)>,
    palette_buttons: Vec<(Palette, ViewHandle<ActionButton>)>,
}

impl Entity for AgentProfileEditorView {
    type Event = AgentProfileEditorEvent;
}

impl AgentProfileEditorView {
    pub fn new(profile: AgentProfile, ctx: &mut ViewContext<Self>) -> Self {
        let editor = ctx.add_typed_action_view(|ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let mut view = EditorView::single_line(
                SingleLineEditorOptions {
                    text: TextOptions::ui_text(None, appearance),
                    ..Default::default()
                },
                ctx,
            );
            view.set_placeholder_text("Nombre del agente", ctx);
            view.set_buffer_text(&profile.display_name, ctx);
            view
        });
        ctx.subscribe_to_view(&editor, |me, _, event, ctx| {
            if let EditorEvent::Edited(_) = event {
                let name = me.editor.read(ctx, |editor, ctx| editor.buffer_text(ctx));
                me.draft.set_display_name(name);
                ctx.notify();
            } else if let EditorEvent::Escape = event {
                ctx.emit(AgentProfileEditorEvent::Cancelled);
            }
        });
        let save_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Guardar", PrimaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(AgentProfileEditorAction::Save))
        });
        let cancel_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Cancelar", SecondaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(AgentProfileEditorAction::Cancel))
        });
        let pick_png_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Subir avatar PNG…", SecondaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(AgentProfileEditorAction::PickAvatarPng))
        });
        let clear_png_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Quitar avatar", NakedTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(AgentProfileEditorAction::ClearAvatarPng))
        });
        let icon_buttons = AvatarKind::ALL
            .into_iter()
            .map(|icon| {
                let handle = ctx.add_typed_action_view(move |_| {
                    ActionButton::new(icon.label(), NakedTheme)
                        .with_size(ButtonSize::Small)
                        .on_click(move |ctx| {
                            ctx.dispatch_typed_action(AgentProfileEditorAction::SetIcon(icon))
                        })
                });
                (icon, handle)
            })
            .collect::<Vec<_>>();
        let palette_buttons = Palette::ALL
            .into_iter()
            .map(|palette| {
                let handle = ctx.add_typed_action_view(move |_| {
                    ActionButton::new(palette.label(), NakedTheme)
                        .with_size(ButtonSize::Small)
                        .on_click(move |ctx| {
                            ctx.dispatch_typed_action(AgentProfileEditorAction::SetPalette(
                                palette,
                            ))
                        })
                });
                (palette, handle)
            })
            .collect::<Vec<_>>();

        let mut me = Self {
            editor,
            draft: AgentProfileEditor::begin(&profile),
            save_button,
            cancel_button,
            pick_png_button,
            clear_png_button,
            icon_buttons,
            palette_buttons,
        };
        me.sync_selection_chrome(ctx);
        me
    }

    fn open_avatar_picker(&mut self, ctx: &mut ViewContext<Self>) {
        use std::path::PathBuf;
        use warpui::platform::file_picker::{FilePickerConfiguration, FileType};

        let profile_key = self.draft.draft().key();
        ctx.open_file_picker(
            move |result, ctx| match result {
                Ok(paths) => {
                    if let Some(path) = paths.into_iter().next() {
                        let source = PathBuf::from(path);
                        match AgentProfile::import_avatar_image(&profile_key, &source) {
                            Ok(dest) => {
                                ctx.dispatch_typed_action(
                                    &AgentProfileEditorAction::SetAvatarPngPath(
                                        dest.display().to_string(),
                                    ),
                                );
                            }
                            Err(err) => {
                                log::warn!("Failed to import avatar PNG: {err}");
                            }
                        }
                    }
                }
                Err(err) => {
                    log::warn!("Avatar file picker error: {err}");
                }
            },
            FilePickerConfiguration::new().set_allowed_file_types(vec![FileType::Image]),
        );
    }

    pub fn save(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(AgentProfileEditorEvent::Saved(self.draft.snapshot()));
    }

    #[allow(dead_code)] // Reused by the vertical-tabs profile affordance.
    pub fn set_profile(&mut self, profile: AgentProfile, ctx: &mut ViewContext<Self>) {
        self.draft = AgentProfileEditor::begin(&profile);
        self.editor.update(ctx, |editor, ctx| {
            editor.clear_buffer_and_reset_undo_stack(ctx);
            editor.set_buffer_text(&profile.display_name, ctx);
        });
        self.sync_selection_chrome(ctx);
        ctx.notify();
    }

    pub fn cancel(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(AgentProfileEditorEvent::Cancelled);
    }

    /// Make the selected icon/color obvious: Primary theme + checkmark.
    /// Without this, clicks update draft state but every button looks identical.
    fn sync_selection_chrome(&mut self, ctx: &mut ViewContext<Self>) {
        let selected_icon = self.draft.draft().icon;
        let selected_palette = self.draft.draft().palette;
        for (icon, button) in &self.icon_buttons {
            let selected = *icon == selected_icon;
            button.update(ctx, |button, ctx| {
                button.set_active(selected, ctx);
                if selected {
                    button.set_theme(PrimaryTheme, ctx);
                    button.set_label(format!("✓ {}", icon.label()), ctx);
                } else {
                    button.set_theme(NakedTheme, ctx);
                    button.set_label(icon.label(), ctx);
                }
            });
        }
        for (palette, button) in &self.palette_buttons {
            let selected = *palette == selected_palette;
            button.update(ctx, |button, ctx| {
                button.set_active(selected, ctx);
                if selected {
                    button.set_theme(PrimaryTheme, ctx);
                    button.set_label(format!("✓ {}", palette.label()), ctx);
                } else {
                    button.set_theme(NakedTheme, ctx);
                    button.set_label(palette.label(), ctx);
                }
            });
        }
    }

    fn render_preview(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let draft = self.draft.draft();
        let content = if let Some(path) = draft.avatar_image_path.as_ref() {
            AvatarContent::LocalImage {
                path: path.clone(),
                display_name: draft.display_name.clone(),
            }
        } else {
            match draft.icon {
                AvatarKind::Initial => AvatarContent::DisplayName(draft.display_name.clone()),
                AvatarKind::Assistant => AvatarContent::Icon(CoreIcon::AiAssistant),
                AvatarKind::Code => AvatarContent::Icon(CoreIcon::Code2),
                AvatarKind::Terminal => AvatarContent::Icon(CoreIcon::Terminal),
            }
        };
        let background = match draft.palette {
            Palette::Blue => theme.ansi_fg_blue(),
            Palette::Green => theme.ansi_fg_green(),
            Palette::Orange => theme.ansi_fg_yellow(),
            Palette::Purple => theme.ansi_fg_magenta(),
            Palette::Red => theme.ansi_fg_red(),
        };
        let avatar = Avatar::new(
            content,
            UiComponentStyles {
                width: Some(PREVIEW_AVATAR_SIZE),
                height: Some(PREVIEW_AVATAR_SIZE),
                font_size: Some(12.),
                font_family_id: Some(appearance.monospace_font_family()),
                font_weight: Some(Weight::Bold),
                font_color: Some(theme.surface_1().into()),
                background: Some(background.into()),
                border_radius: Some(CornerRadius::with_all(Radius::Pixels(
                    PREVIEW_AVATAR_SIZE / 2.,
                ))),
                ..Default::default()
            },
        )
        .build()
        .finish();

        let subtitle = if draft.avatar_image_path.is_some() {
            format!("PNG personalizado · {}", draft.palette.label())
        } else {
            format!("{} · {}", draft.icon.label(), draft.palette.label())
        };

        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(10.)
            .with_child(avatar)
            .with_child(
                Flex::column()
                    .with_main_axis_size(MainAxisSize::Min)
                    .with_child(
                        Text::new(
                            if draft.display_name.is_empty() {
                                "Agent name".into()
                            } else {
                                draft.display_name.clone()
                            },
                            appearance.ui_font_family(),
                            14.,
                        )
                        .finish(),
                    )
                    .with_child(
                        Text::new(subtitle, appearance.ui_font_family(), 11.)
                            .with_color(theme.sub_text_color(theme.background()).into())
                            .finish(),
                    )
                    .finish(),
            )
            .finish()
    }
}

impl View for AgentProfileEditorView {
    fn ui_name() -> &'static str {
        "AgentProfileEditor"
    }
    fn on_focus(&mut self, focus_ctx: &FocusContext, ctx: &mut ViewContext<Self>) {
        if focus_ctx.is_self_focused() {
            ctx.focus(&self.editor);
        }
    }
    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let icons = Flex::row()
            .with_spacing(4.)
            .with_children(
                self.icon_buttons
                    .iter()
                    .map(|(_, button)| ChildView::new(button).finish()),
            )
            .finish();
        let palettes = Flex::row()
            .with_spacing(4.)
            .with_children(
                self.palette_buttons
                    .iter()
                    .map(|(_, button)| ChildView::new(button).finish()),
            )
            .finish();
        let actions = Flex::row()
            .with_spacing(8.)
            .with_child(ChildView::new(&self.cancel_button).finish())
            .with_child(ChildView::new(&self.save_button).finish())
            .finish();
        Container::new(
            Flex::column()
                .with_main_axis_size(MainAxisSize::Min)
                .with_spacing(10.)
                .with_child(
                    Text::new("Personalizar agente", appearance.ui_font_family(), 16.).finish(),
                )
                .with_child(self.render_preview(appearance))
                .with_child(ChildView::new(&self.editor).finish())
                .with_child(
                    Text::new("Ícono", appearance.ui_font_family(), 12.).finish(),
                )
                .with_child(icons)
                .with_child(
                    Text::new("Avatar PNG / JPG", appearance.ui_font_family(), 12.).finish(),
                )
                .with_child(
                    Flex::row()
                        .with_spacing(6.)
                        .with_child(ChildView::new(&self.pick_png_button).finish())
                        .with_child(ChildView::new(&self.clear_png_button).finish())
                        .finish(),
                )
                .with_child(Text::new("Color", appearance.ui_font_family(), 12.).finish())
                .with_child(palettes)
                .with_child(
                    Text::new(
                        "Tocá el avatar en el acordeón para volver a editar. Guardar aplica nombre, logo y color en el rail.",
                        appearance.ui_font_family(),
                        10.,
                    )
                    .with_color(theme.sub_text_color(theme.background()).into())
                    .finish(),
                )
                .with_child(actions)
                .finish(),
        )
        .with_padding(warpui::elements::Padding::uniform(4.))
        .finish()
    }
}

impl TypedActionView for AgentProfileEditorView {
    type Action = AgentProfileEditorAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            AgentProfileEditorAction::Save => self.save(ctx),
            AgentProfileEditorAction::Cancel => self.cancel(ctx),
            AgentProfileEditorAction::SetIcon(icon) => {
                // Built-in icon replaces custom PNG.
                self.draft.set_avatar_image_path(None);
                self.draft.set_icon(*icon);
                self.sync_selection_chrome(ctx);
                ctx.notify();
            }
            AgentProfileEditorAction::SetPalette(palette) => {
                self.draft.set_palette(*palette);
                self.sync_selection_chrome(ctx);
                ctx.notify();
            }
            AgentProfileEditorAction::PickAvatarPng => {
                self.open_avatar_picker(ctx);
            }
            AgentProfileEditorAction::ClearAvatarPng => {
                self.draft.set_avatar_image_path(None);
                ctx.notify();
            }
            AgentProfileEditorAction::SetAvatarPngPath(path) => {
                self.draft.set_avatar_image_path(Some(path.clone()));
                ctx.notify();
            }
        }
    }
}

