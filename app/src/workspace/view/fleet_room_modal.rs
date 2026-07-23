//! Fleet Room modal — multi-agent group chat (BLOOME-like).
//!
//! Implemented as a full-window View so layout gets finite window constraints
//! (stacking a raw Flex Max element on Workspace was panicking).

use pathfinder_color::ColorU;
use pathfinder_geometry::vector::vec2f;
use warpui::elements::{
    Align, Border, ChildAnchor, ChildView, ConstrainedBox, Container, CornerRadius,
    CrossAxisAlignment, Element, Flex, Hoverable, MainAxisAlignment, MainAxisSize,
    MouseStateHandle, OffsetPositioning, Padding, ParentAnchor, ParentElement, ParentOffsetBounds,
    Radius, Stack, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::keymap::FixedBinding;
use warpui::platform::Cursor;
use warpui::text_layout::ClipConfig;
use warpui::{
    AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use crate::appearance::Appearance;
use crate::editor::{
    EditorView, Event as EditorEvent, PropagateAndNoOpNavigationKeys, SingleLineEditorOptions,
    TextOptions,
};
use crate::workspace::WorkspaceAction;
use crate::workspace::agent_fleet_room::{
    FleetAuthor, FleetMemberStatus, FleetMessage, FleetMessageKind, FleetRoom, mention_token,
};
use crate::workspace::agent_provider_hub::{
    AgentProviderHubPrefs, AgentProviderId, AgentProviderLaunchMode, command_on_path,
};

const CARD_W: f32 = 900.;
const CARD_H: f32 = 560.;
const SIDEBAR_W: f32 = 210.;
const RADIUS: f32 = 8.;

pub fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;
    app.register_fixed_bindings([FixedBinding::new(
        "escape",
        FleetRoomModalAction::Close,
        id!("FleetRoomModal"),
    )]);
}

#[derive(Clone, Debug)]
pub enum FleetRoomModalAction {
    Close,
    Send,
    SelectMember(AgentProviderId),
    LaunchMember(AgentProviderId),
    InsertMention(String),
}

#[derive(Clone, Debug)]
pub enum FleetRoomModalEvent {
    Close,
    /// Workspace should launch this shell line in a new tab.
    LaunchShell { command: String },
}

pub struct FleetRoomModal {
    room: FleetRoom,
    composer: ViewHandle<EditorView>,
    close_ms: MouseStateHandle,
    send_ms: MouseStateHandle,
    mention_ms: Vec<MouseStateHandle>,
    member_ms: Vec<MouseStateHandle>,
    launch_ms: Vec<MouseStateHandle>,
}

impl FleetRoomModal {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let composer = ctx.add_typed_action_view(|ctx| {
            let appearance = Appearance::as_ref(ctx);
            let mut editor = EditorView::single_line(
                SingleLineEditorOptions {
                    text: TextOptions::ui_text(Some(appearance.ui_font_size()), appearance),
                    select_all_on_focus: false,
                    clear_selections_on_blur: false,
                    propagate_and_no_op_vertical_navigation_keys:
                        PropagateAndNoOpNavigationKeys::Always,
                    ..Default::default()
                },
                ctx,
            );
            editor.set_placeholder_text("Mensaje · @claude @codex @all…", ctx);
            editor
        });
        ctx.subscribe_to_view(&composer, |me, _, event, ctx| {
            if matches!(event, EditorEvent::Enter) {
                me.send(ctx);
            }
        });

        Self {
            room: FleetRoom::new_local(),
            composer,
            close_ms: MouseStateHandle::default(),
            send_ms: MouseStateHandle::default(),
            mention_ms: (0..4).map(|_| MouseStateHandle::default()).collect(),
            member_ms: (0..12).map(|_| MouseStateHandle::default()).collect(),
            launch_ms: (0..12).map(|_| MouseStateHandle::default()).collect(),
        }
    }

    /// Refresh members from hub prefs + seed welcome when opening.
    pub fn prepare_open(&mut self, ctx: &mut ViewContext<Self>) {
        let prefs = AgentProviderHubPrefs::load_default();
        let enabled: Vec<AgentProviderId> = AgentProviderId::ALL
            .into_iter()
            .filter(|id| prefs.is_enabled(*id))
            .collect();
        let enabled = if enabled.is_empty() {
            AgentProviderId::DEFAULT_ENABLED.to_vec()
        } else {
            enabled
        };
        self.room.sync_members(enabled, |id| {
            id.detect_commands().iter().any(|c| command_on_path(c))
        });
        let now_ms = now_ms();
        self.room.seed_welcome_if_needed(now_ms);
        ctx.focus(&self.composer);
        ctx.notify();
    }

    fn send(&mut self, ctx: &mut ViewContext<Self>) {
        let draft = self.composer.as_ref(ctx).buffer_text(ctx);
        let Some(plan) = self.room.send_user_message(&draft, now_ms()) else {
            return;
        };
        self.composer.update(ctx, |ed, ctx| {
            ed.set_buffer_text("", ctx);
        });
        for route in plan.routes {
            let command =
                crate::workspace::agent_fleet_room::launch_shell_for_route(route.provider, &route.prompt);
            ctx.emit(FleetRoomModalEvent::LaunchShell { command });
        }
        ctx.notify();
    }

    fn insert_mention(&mut self, token: &str, ctx: &mut ViewContext<Self>) {
        let current = self.composer.as_ref(ctx).buffer_text(ctx);
        let mention = format!("@{token}");
        let next = if current.trim().is_empty() {
            format!("{mention} ")
        } else if current.contains(&mention) {
            current
        } else {
            format!("{current} {mention} ")
        };
        self.composer.update(ctx, |ed, ctx| {
            ed.set_buffer_text(&next, ctx);
        });
        ctx.focus(&self.composer);
        ctx.notify();
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Entity for FleetRoomModal {
    type Event = FleetRoomModalEvent;
}

impl View for FleetRoomModal {
    fn ui_name() -> &'static str {
        "FleetRoomModal"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let font = appearance.ui_font_family();
        let main = ColorU::new(235, 235, 240, 255);
        let sub = ColorU::new(160, 160, 170, 255);

        let header = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Flex::column()
                    .with_spacing(2.)
                    .with_child(
                        Text::new_inline(self.room.title.clone(), font, 16.)
                            .with_color(main)
                            .with_style(Properties::default().weight(Weight::Bold))
                            .finish(),
                    )
                    .with_child(
                        Text::new_inline(
                            format!(
                                "{} agents · sala multiagent local",
                                self.room.members.len()
                            ),
                            font,
                            11.,
                        )
                        .with_color(sub)
                        .finish(),
                    )
                    .finish(),
            )
            .with_child(chip(
                "Cerrar",
                self.close_ms.clone(),
                FleetRoomModalAction::Close,
                font,
                false,
            ))
            .finish();

        let mut members_col = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(6.)
            .with_child(
                Text::new_inline(
                    format!("Members ({})", self.room.members.len()),
                    font,
                    12.,
                )
                .with_color(sub)
                .finish(),
            );

        for (i, member) in self.room.members.iter().enumerate() {
            let selected = self.room.selected_provider == Some(member.provider);
            let provider = member.provider;
            let ms = self
                .member_ms
                .get(i)
                .cloned()
                .unwrap_or_default();
            let lms = self
                .launch_ms
                .get(i)
                .cloned()
                .unwrap_or_default();
            let title = member.slot_label.clone();
            let meta = format!(
                "{} · {} · @{}",
                if member.ready {
                    "listo"
                } else {
                    "instalar CLI"
                },
                member_status_label(member.status, member.ready),
                mention_token(provider)
            );
            let row = Hoverable::new(ms, move |_| {
                Container::new(
                    Flex::column()
                        .with_spacing(4.)
                        .with_child(
                            Text::new_inline(title.clone(), font, 12.)
                                .with_color(main)
                                .with_clip(ClipConfig::ellipsis())
                                .finish(),
                        )
                        .with_child(
                            Text::new_inline(meta.clone(), font, 10.)
                                .with_color(sub)
                                .with_clip(ClipConfig::ellipsis())
                                .finish(),
                        )
                        .finish(),
                )
                .with_padding(Padding::uniform(8.))
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(RADIUS)))
                .with_background_color(if selected {
                    ColorU::new(80, 120, 200, 50)
                } else {
                    ColorU::new(255, 255, 255, 8)
                })
                .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(FleetRoomModalAction::SelectMember(provider));
            })
            .finish();
            members_col = members_col
                .with_child(row)
                .with_child(chip(
                    "Abrir CLI",
                    lms,
                    FleetRoomModalAction::LaunchMember(provider),
                    font,
                    false,
                ));
        }

        let mut thread = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(6.);
        if self.room.messages.is_empty() {
            thread = thread.with_child(
                Text::new_inline(
                    "Sin mensajes. Usá @claude @codex @all…",
                    font,
                    12.,
                )
                .with_color(sub)
                .finish(),
            );
        }
        let start = self.room.messages.len().saturating_sub(20);
        for msg in &self.room.messages[start..] {
            thread = thread.with_child(bubble(msg, font, main, sub));
        }

        let editor = ConstrainedBox::new(
            Container::new(ChildView::new(&self.composer).finish())
                .with_padding(Padding::uniform(8.))
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(RADIUS)))
                .with_border(Border::all(1.).with_border_fill(theme.outline()))
                .with_background_color(ColorU::new(255, 255, 255, 12))
                .finish(),
        )
        .with_height(40.)
        .finish();

        let mentions = [
            ("@all", "all"),
            ("@claude", "claude"),
            ("@codex", "codex"),
            ("@grok", "grok"),
        ];
        let mut mention_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Min)
            .with_spacing(6.);
        for (i, (label, token)) in mentions.iter().enumerate() {
            let ms = self.mention_ms.get(i).cloned().unwrap_or_default();
            mention_row = mention_row.with_child(chip(
                label,
                ms,
                FleetRoomModalAction::InsertMention((*token).into()),
                font,
                false,
            ));
        }

        let composer_row = Flex::column()
            .with_spacing(8.)
            .with_child(mention_row.finish())
            .with_child(
                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(8.)
                    .with_child(editor)
                    .with_child(chip(
                        "Enviar",
                        self.send_ms.clone(),
                        FleetRoomModalAction::Send,
                        font,
                        true,
                    ))
                    .finish(),
            )
            .with_child(
                Text::new_inline(
                    "Enviar lanza CLIs reales en tabs nuevos.",
                    font,
                    10.,
                )
                .with_color(sub)
                .finish(),
            )
            .finish();

        let main_col = Flex::column()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(10.)
            .with_child(header)
            .with_child(
                ConstrainedBox::new(thread.finish())
                    .with_max_height(CARD_H - 240.)
                    .finish(),
            )
            .with_child(composer_row)
            .finish();

        let body = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(12.)
            .with_child(
                ConstrainedBox::new(
                    Container::new(members_col.finish())
                        .with_padding(Padding::uniform(10.))
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(RADIUS)))
                        .with_background_color(ColorU::new(0, 0, 0, 40))
                        .finish(),
                )
                .with_width(SIDEBAR_W)
                .finish(),
            )
            .with_child(
                Container::new(main_col)
                    .with_padding(Padding::uniform(10.))
                    .finish(),
            )
            .finish();

        let card = Container::new(
            ConstrainedBox::new(body)
                .with_width(CARD_W)
                .with_height(CARD_H)
                .finish(),
        )
        .with_background(theme.surface_1())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(12.)))
        .with_border(Border::all(1.).with_border_fill(theme.outline()))
        .with_padding(Padding::uniform(4.))
        .finish();

        let mut stack = Stack::new();
        stack.add_positioned_child(
            card,
            OffsetPositioning::offset_from_parent(
                vec2f(0., 0.),
                ParentOffsetBounds::WindowByPosition,
                ParentAnchor::Center,
                ChildAnchor::Center,
            ),
        );

        Container::new(Align::new(stack.finish()).finish())
            .with_background_color(ColorU::new(0, 0, 0, 160))
            .finish()
    }
}

impl TypedActionView for FleetRoomModal {
    type Action = FleetRoomModalAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            FleetRoomModalAction::Close => {
                ctx.emit(FleetRoomModalEvent::Close);
            }
            FleetRoomModalAction::Send => self.send(ctx),
            FleetRoomModalAction::SelectMember(p) => {
                self.room.select_member(Some(*p));
                ctx.notify();
            }
            FleetRoomModalAction::LaunchMember(p) => {
                let command = p.launch_shell_line(AgentProviderLaunchMode::NewSession);
                ctx.emit(FleetRoomModalEvent::LaunchShell { command });
            }
            FleetRoomModalAction::InsertMention(token) => {
                self.insert_mention(token, ctx);
            }
        }
    }
}

fn bubble(
    msg: &FleetMessage,
    font: warpui::fonts::FamilyId,
    main: ColorU,
    sub: ColorU,
) -> Box<dyn Element> {
    let label = match &msg.author {
        FleetAuthor::User => "Vos".to_string(),
        FleetAuthor::System => "Sistema".to_string(),
        FleetAuthor::Provider(p) => p.display_name().to_string(),
    };
    let tint = match &msg.author {
        FleetAuthor::User => ColorU::new(60, 100, 180, 55),
        FleetAuthor::System => ColorU::new(80, 80, 80, 35),
        FleetAuthor::Provider(p) => provider_tint(*p),
    };
    let body_color = if msg.kind == FleetMessageKind::Route {
        sub
    } else {
        main
    };
    Flex::column()
        .with_spacing(2.)
        .with_child(
            Text::new_inline(label, font, 11.)
                .with_color(sub)
                .finish(),
        )
        .with_child(
            Container::new(
                Text::new_inline(msg.body.clone(), font, 13.)
                    .with_color(body_color)
                    .finish(),
            )
            .with_padding(Padding::uniform(8.))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(RADIUS)))
            .with_background_color(tint)
            .finish(),
        )
        .finish()
}

fn chip(
    label: &str,
    ms: MouseStateHandle,
    action: FleetRoomModalAction,
    font: warpui::fonts::FamilyId,
    primary: bool,
) -> Box<dyn Element> {
    let label = label.to_string();
    Hoverable::new(ms, move |_| {
        let bg = if primary {
            ColorU::new(70, 120, 220, 220)
        } else {
            ColorU::new(255, 255, 255, 18)
        };
        let fg = if primary {
            ColorU::new(255, 255, 255, 255)
        } else {
            ColorU::new(210, 210, 210, 255)
        };
        Container::new(
            Text::new_inline(label.clone(), font, 12.)
                .with_color(fg)
                .finish(),
        )
        .with_padding(
            Padding::uniform(0.)
                .with_top(7.)
                .with_bottom(7.)
                .with_left(12.)
                .with_right(12.),
        )
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(RADIUS)))
        .with_background_color(bg)
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}

fn member_status_label(status: FleetMemberStatus, ready: bool) -> &'static str {
    if !ready {
        return "offline";
    }
    match status {
        FleetMemberStatus::Idle => "idle",
        FleetMemberStatus::Working => "trabajando",
        FleetMemberStatus::NeedsYou => "te necesita",
        FleetMemberStatus::Offline => "offline",
    }
}

fn provider_tint(id: AgentProviderId) -> ColorU {
    match id {
        AgentProviderId::Claude => ColorU::new(200, 140, 80, 55),
        AgentProviderId::Codex => ColorU::new(80, 160, 120, 55),
        AgentProviderId::Grok => ColorU::new(120, 120, 200, 55),
        AgentProviderId::Gemini => ColorU::new(80, 140, 220, 55),
        AgentProviderId::Hermes => ColorU::new(100, 180, 100, 55),
        _ => ColorU::new(100, 100, 100, 50),
    }
}
