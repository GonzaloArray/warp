//! Small, feature-gated editor for an agent's local display identity.

use crate::ai::agent_management::profiles::{
    AgentProfile, AgentProfileEditor, AvatarKind, Palette,
};
use crate::appearance::Appearance;
use crate::editor::Event as EditorEvent;
use crate::editor::{EditorView, SingleLineEditorOptions, TextOptions};
use crate::view_components::action_button::{
    ActionButton, ButtonSize, NakedTheme, PrimaryTheme, SecondaryTheme,
};
use warpui::elements::{ChildView, Container, Flex, MainAxisSize, ParentElement, Text};
use warpui::{
    AppContext, Element, Entity, FocusContext, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

#[derive(Clone, Debug)]
pub enum AgentProfileEditorAction {
    Save,
    Cancel,
    SetIcon(AvatarKind),
    SetPalette(Palette),
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
    icon_buttons: Vec<ViewHandle<ActionButton>>,
    palette_buttons: Vec<ViewHandle<ActionButton>>,
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
            view.set_placeholder_text("Agent name", ctx);
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
            ActionButton::new("Save", PrimaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(AgentProfileEditorAction::Save))
        });
        let cancel_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Cancel", SecondaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(AgentProfileEditorAction::Cancel))
        });
        let icon_buttons = AvatarKind::ALL
            .into_iter()
            .map(|icon| {
                ctx.add_typed_action_view(move |_| {
                    ActionButton::new(icon.label(), NakedTheme)
                        .with_size(ButtonSize::Small)
                        .on_click(move |ctx| {
                            ctx.dispatch_typed_action(AgentProfileEditorAction::SetIcon(icon))
                        })
                })
            })
            .collect();
        let palette_buttons = Palette::ALL
            .into_iter()
            .map(|palette| {
                ctx.add_typed_action_view(move |_| {
                    ActionButton::new(palette.label(), NakedTheme)
                        .with_size(ButtonSize::Small)
                        .on_click(move |ctx| {
                            ctx.dispatch_typed_action(AgentProfileEditorAction::SetPalette(palette))
                        })
                })
            })
            .collect();
        Self {
            editor,
            draft: AgentProfileEditor::begin(&profile),
            save_button,
            cancel_button,
            icon_buttons,
            palette_buttons,
        }
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
    }

    pub fn cancel(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(AgentProfileEditorEvent::Cancelled);
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
        let icons = Flex::row()
            .with_spacing(4.)
            .with_children(
                self.icon_buttons
                    .iter()
                    .map(|button| ChildView::new(button).finish()),
            )
            .finish();
        let palettes = Flex::row()
            .with_spacing(4.)
            .with_children(
                self.palette_buttons
                    .iter()
                    .map(|button| ChildView::new(button).finish()),
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
                .with_child(Text::new("Customize agent", appearance.ui_font_family(), 16.).finish())
                .with_child(warpui::elements::ChildView::new(&self.editor).finish())
                .with_child(Text::new("Avatar", appearance.ui_font_family(), 12.).finish())
                .with_child(icons)
                .with_child(Text::new("Color", appearance.ui_font_family(), 12.).finish())
                .with_child(palettes)
                .with_child(actions)
                .finish(),
        )
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
                self.draft.set_icon(*icon);
                ctx.notify();
            }
            AgentProfileEditorAction::SetPalette(palette) => {
                self.draft.set_palette(*palette);
                ctx.notify();
            }
        }
    }
}
