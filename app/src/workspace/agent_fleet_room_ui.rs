//! Fleet Room overlay UI (BLOOME-like group surface).

use pathfinder_color::ColorU;
use warpui::elements::{
    Border, ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Element, Flex,
    Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle, Padding, ParentElement, Radius,
    Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::platform::Cursor;
use warpui::text_layout::ClipConfig;
use warpui::{AppContext, SingletonEntity, ViewHandle};

use crate::appearance::Appearance;
use crate::editor::EditorView;
use crate::workspace::WorkspaceAction;
use crate::workspace::agent_fleet_room::{
    FleetAuthor, FleetMemberStatus, FleetMessage, FleetMessageKind, FleetRoom, mention_token,
};
use crate::workspace::agent_provider_hub::AgentProviderId;

const SIDEBAR_W: f32 = 220.;
const CARD_RADIUS: f32 = 8.;

pub(crate) fn render_fleet_room_overlay(
    room: &FleetRoom,
    composer: Option<&ViewHandle<EditorView>>,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let main = ColorU::new(235, 235, 240, 255);
    let sub = ColorU::new(160, 160, 170, 255);
    let font = appearance.ui_font_family();
    let header = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(
            Flex::column()
                .with_spacing(2.)
                .with_child(
                    Text::new_inline(room.title.clone(), font, 16.)
                        .with_color(main)
                        .with_style(Properties::default().weight(Weight::Bold))
                        .finish(),
                )
                .with_child(
                    Text::new_inline(
                        format!("{} agents · sala multiagent local", room.members.len()),
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
            WorkspaceAction::CloseFleetRoom,
            font,
            false,
        ))
        .finish();

    let mut members = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(6.)
        .with_child(
            Text::new_inline(format!("Members ({})", room.members.len()), font, 12.)
                .with_color(sub)
                .finish(),
        );

    for member in &room.members {
        let selected = room.selected_provider == Some(member.provider);
        let provider = member.provider;
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
        let title_c = title.clone();
        let meta_c = meta.clone();
        let row = Hoverable::new(MouseStateHandle::default(), move |_| {
            Container::new(
                Flex::column()
                    .with_spacing(4.)
                    .with_child(
                        Text::new_inline(title_c.clone(), font, 12.)
                            .with_color(main)
                            .with_clip(ClipConfig::ellipsis())
                            .finish(),
                    )
                    .with_child(
                        Text::new_inline(meta_c.clone(), font, 10.)
                            .with_color(sub)
                            .with_clip(ClipConfig::ellipsis())
                            .finish(),
                    )
                    .with_child(chip(
                        "Abrir CLI",
                        WorkspaceAction::FleetRoomLaunchMember { provider },
                        font,
                        false,
                    ))
                    .finish(),
            )
            .with_padding(Padding::uniform(8.))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
            .with_background_color(if selected {
                ColorU::new(80, 120, 200, 50)
            } else {
                ColorU::new(255, 255, 255, 8)
            })
            .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(WorkspaceAction::FleetRoomSelectMember { provider });
        })
        .finish();
        members = members.with_child(row);
    }

    let mut thread = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(8.);

    if room.messages.is_empty() {
        thread = thread.with_child(
            Text::new_inline(
                "Sin mensajes. Escribí abajo y usá @claude @codex @all…",
                font,
                12.,
            )
            .with_color(sub)
            .finish(),
        );
    }
    for msg in &room.messages {
        thread = thread.with_child(bubble(msg, font, main, sub));
    }

    let editor_el: Box<dyn Element> = if let Some(handle) = composer {
        Container::new(ChildView::new(handle).finish())
            .with_padding(Padding::uniform(8.))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
            .with_border(Border::all(1.).with_border_fill(theme.outline()))
            .with_background_color(ColorU::new(255, 255, 255, 10))
            .finish()
    } else {
        Text::new_inline("Composer…", font, 12.)
            .with_color(sub)
            .finish()
    };

    let composer_row = Flex::column()
        .with_spacing(8.)
        .with_child(
            Flex::row()
                .with_spacing(6.)
                .with_child(chip(
                    "@all",
                    WorkspaceAction::FleetRoomInsertMention {
                        token: "all".into(),
                    },
                    font,
                    false,
                ))
                .with_child(chip(
                    "@claude",
                    WorkspaceAction::FleetRoomInsertMention {
                        token: "claude".into(),
                    },
                    font,
                    false,
                ))
                .with_child(chip(
                    "@codex",
                    WorkspaceAction::FleetRoomInsertMention {
                        token: "codex".into(),
                    },
                    font,
                    false,
                ))
                .with_child(chip(
                    "@grok",
                    WorkspaceAction::FleetRoomInsertMention {
                        token: "grok".into(),
                    },
                    font,
                    false,
                ))
                .finish(),
        )
        .with_child(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(8.)
                .with_child(
                    ConstrainedBox::new(editor_el)
                        .with_min_height(36.)
                        .with_max_height(48.)
                        .finish(),
                )
                .with_child(chip(
                    "Enviar",
                    WorkspaceAction::FleetRoomSend,
                    font,
                    true,
                ))
                .finish(),
        )
        .with_child(
            Text::new_inline(
                "Enviar rutea a CLIs reales (tabs). @ elige el agent.",
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
        .with_child(thread.finish())
        .with_child(composer_row)
        .finish();

    let body = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.)
        .with_child(
            ConstrainedBox::new(
                Container::new(members.finish())
                    .with_padding(Padding::uniform(12.))
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
                    .with_background_color(ColorU::new(0, 0, 0, 40))
                    .finish(),
            )
            .with_width(SIDEBAR_W)
            .finish(),
        )
        .with_child(Container::new(main_col).with_padding(Padding::uniform(12.)).finish())
        .finish();

    let card = ConstrainedBox::new(
        Container::new(body)
            .with_padding(Padding::uniform(4.))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(12.)))
            .with_border(Border::all(1.).with_border_fill(theme.outline()))
            .with_background(theme.surface_1())
            .finish(),
    )
    .with_width(960.)
    .with_height(620.)
    .finish();

    Container::new(card)
        .with_padding(Padding::uniform(28.))
        .with_background_color(ColorU::new(0, 0, 0, 150))
        .finish()
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
        .with_cross_axis_alignment(CrossAxisAlignment::Start)
        .with_spacing(3.)
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
            .with_padding(Padding::uniform(10.))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
            .with_background_color(tint)
            .finish(),
        )
        .finish()
}

fn chip(
    label: &str,
    action: WorkspaceAction,
    font: warpui::fonts::FamilyId,
    primary: bool,
) -> Box<dyn Element> {
    let label = label.to_string();
    Hoverable::new(MouseStateHandle::default(), move |_| {
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
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
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

