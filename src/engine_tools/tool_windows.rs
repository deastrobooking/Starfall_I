//! Reusable movable, minimizable tool windows for creator screens.
//!
//! A tool window is an absolutely-positioned panel with a drag handle (its
//! title bar), a minimize/restore button, and a scrollable content container.
//! The framework is screen-agnostic: the Starfall Forge level workspace and
//! the Creature Forge both build their panels through [`spawn_tool_window`],
//! and any future creator screen can join by spawning windows under its own
//! UI root.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Window chrome state. Lives on the window root node.
#[derive(Component, Debug, Default)]
pub struct ToolWindow {
    pub minimized: bool,
    drag: Option<DragState>,
    requested_size: Vec2,
}

#[derive(Debug, Clone, Copy)]
struct DragState {
    pointer_start: Vec2,
    window_start: Vec2,
}

/// The draggable title bar button of one window.
#[derive(Component, Debug, Clone, Copy)]
pub struct ToolWindowTitleBar {
    pub window: Entity,
}

/// The minimize/restore button of one window.
#[derive(Component, Debug, Clone, Copy)]
pub struct ToolWindowMinimizeButton {
    pub window: Entity,
}

/// The collapsible content container of one window.
#[derive(Component, Debug, Clone, Copy)]
pub struct ToolWindowContent {
    pub window: Entity,
}

/// Text glyph on the minimize button ("—" open, "▣" minimized).
#[derive(Component, Debug, Clone, Copy)]
struct ToolWindowMinimizeGlyph {
    window: Entity,
}

/// Monotonic sibling z-order: grabbing a window raises it above the others.
#[derive(Resource, Default)]
struct ToolWindowRaiseOrder(i32);

pub struct ToolWindowsPlugin;

impl Plugin for ToolWindowsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ToolWindowRaiseOrder>().add_systems(
            Update,
            (
                tool_window_drag_system,
                tool_window_fit_system,
                tool_window_minimize_system,
            )
                .chain(),
        );
    }
}

/// Keep at least this much of a window reachable inside the app window so a
/// drag can never strand the title bar off-screen.
const DRAG_MARGIN: Vec2 = Vec2::new(120.0, 40.0);

/// Where a dragged window lands for a given pointer position, clamped so the
/// title bar stays reachable.
fn drag_target(window_start: Vec2, pointer_start: Vec2, pointer: Vec2, bounds: Vec2) -> Vec2 {
    let target = window_start + (pointer - pointer_start);
    Vec2::new(
        target.x.clamp(0.0, (bounds.x - DRAG_MARGIN.x).max(0.0)),
        target.y.clamp(0.0, (bounds.y - DRAG_MARGIN.y).max(0.0)),
    )
}

fn px_or_zero(value: Val) -> f32 {
    match value {
        Val::Px(px) => px,
        _ => 0.0,
    }
}

fn tool_window_drag_system(
    mut commands: Commands,
    mut raise_order: ResMut<ToolWindowRaiseOrder>,
    primary: Query<&Window, With<PrimaryWindow>>,
    bars: Query<(&Interaction, &ToolWindowTitleBar), With<Button>>,
    mut windows: Query<(&mut ToolWindow, &mut Node)>,
) {
    let Ok(primary) = primary.single() else {
        return;
    };
    let cursor = primary.cursor_position();
    let bounds = Vec2::new(primary.width(), primary.height());
    for (interaction, bar) in &bars {
        let Ok((mut window, mut node)) = windows.get_mut(bar.window) else {
            continue;
        };
        if *interaction != Interaction::Pressed {
            if window.drag.is_some() {
                window.drag = None;
            }
            continue;
        }
        let Some(cursor) = cursor else {
            continue;
        };
        match window.drag {
            None => {
                window.drag = Some(DragState {
                    pointer_start: cursor,
                    window_start: Vec2::new(px_or_zero(node.left), px_or_zero(node.top)),
                });
                // Grabbing a window raises it above its sibling windows.
                raise_order.0 += 1;
                commands.entity(bar.window).insert(ZIndex(raise_order.0));
            }
            Some(drag) => {
                let target = drag_target(drag.window_start, drag.pointer_start, cursor, bounds);
                node.left = Val::Px(target.x);
                node.top = Val::Px(target.y);
            }
        }
    }
}

fn tool_window_fit_system(
    primary: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    mut windows: Query<(&ToolWindow, &mut Node)>,
) {
    let Ok(primary) = primary.single() else {
        return;
    };
    let scale = ui_scale.as_deref().map_or(1.0, |scale| scale.0).max(0.01);
    let logical_bounds = Vec2::new(primary.width(), primary.height()) / scale;
    for (window, mut node) in windows.iter_mut() {
        if window.requested_size == Vec2::ZERO {
            continue;
        }
        let current = Vec2::new(px_or_zero(node.left), px_or_zero(node.top));
        let fitted = fit_window_origin(current, window.requested_size, logical_bounds);
        node.left = Val::Px(fitted.x);
        node.top = Val::Px(fitted.y);
    }
}

fn fit_window_origin(origin: Vec2, size: Vec2, bounds: Vec2) -> Vec2 {
    Vec2::new(
        origin.x.clamp(0.0, (bounds.x - size.x).max(0.0)),
        origin.y.clamp(0.0, (bounds.y - size.y).max(0.0)),
    )
}

fn tool_window_minimize_system(
    buttons: Query<(&Interaction, &ToolWindowMinimizeButton), (Changed<Interaction>, With<Button>)>,
    mut windows: Query<&mut ToolWindow>,
    mut contents: Query<(&mut Node, &ToolWindowContent)>,
    mut glyphs: Query<(&mut Text, &ToolWindowMinimizeGlyph)>,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Ok(mut window) = windows.get_mut(button.window) else {
            continue;
        };
        window.minimized = !window.minimized;
        let minimized = window.minimized;
        for (mut node, content) in contents.iter_mut() {
            if content.window == button.window {
                node.display = if minimized {
                    Display::None
                } else {
                    Display::Flex
                };
            }
        }
        for (mut text, glyph) in glyphs.iter_mut() {
            if glyph.window == button.window {
                *text = Text::new(if minimized { "▣" } else { "—" });
            }
        }
    }
}

/// Visual defaults shared by every tool window.
pub struct ToolWindowStyle {
    pub accent: Color,
    pub background: Color,
    pub width: f32,
    pub content_height: Val,
}

impl Default for ToolWindowStyle {
    fn default() -> Self {
        Self {
            accent: Color::srgb(0.14, 0.42, 0.62),
            background: Color::srgba(0.035, 0.05, 0.085, 0.94),
            width: 286.0,
            content_height: Val::Px(540.0),
        }
    }
}

/// Spawn a movable, minimizable window under `parent`. Returns the window
/// root entity; extra components for the scrollable content node come from
/// `content_extras` (e.g. panel markers + `ScrollPosition`), and the caller
/// fills the content through `content`.
pub fn spawn_tool_window(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    position: Vec2,
    style: ToolWindowStyle,
    content_extras: impl Bundle,
    content: impl FnOnce(&mut ChildSpawnerCommands),
) -> Entity {
    let mut window_entity = Entity::PLACEHOLDER;
    let title = title.to_string();
    let content_height = match style.content_height {
        Val::Px(height) => height,
        _ => 540.0,
    };
    parent
        .spawn((
            ToolWindow {
                requested_size: Vec2::new(style.width, content_height + 36.0),
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(position.x),
                top: Val::Px(position.y),
                width: Val::Px(style.width),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(style.background),
            BorderColor::all(style.accent),
        ))
        .with_children(|window| {
            window_entity = window.target_entity();
            window
                .spawn((
                    Button,
                    ToolWindowTitleBar {
                        window: window_entity,
                    },
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(36.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Val::Px(8.0),
                        ..default()
                    },
                    BackgroundColor(style.accent.with_alpha(0.35)),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Text::new(title),
                        TextFont {
                            font_size: FontSize::Px(16.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.78, 0.25)),
                    ));
                    bar.spawn((
                        Button,
                        ToolWindowMinimizeButton {
                            window: window_entity,
                        },
                        Node {
                            min_width: Val::Px(34.0),
                            min_height: Val::Px(34.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new("—"),
                            TextFont {
                                font_size: FontSize::Px(14.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            ToolWindowMinimizeGlyph {
                                window: window_entity,
                            },
                        ));
                    });
                });
            window
                .spawn((
                    ToolWindowContent {
                        window: window_entity,
                    },
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(5.0),
                        padding: UiRect::all(Val::Px(12.0)),
                        height: style.content_height,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    content_extras,
                ))
                .with_children(content);
        });
    window_entity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_target_follows_pointer_and_clamps_to_bounds() {
        let bounds = Vec2::new(1280.0, 720.0);
        let start = Vec2::new(100.0, 80.0);
        let pointer_start = Vec2::new(140.0, 90.0);

        let moved = drag_target(start, pointer_start, Vec2::new(240.0, 190.0), bounds);
        assert_eq!(moved, Vec2::new(200.0, 180.0));

        let clamped_low = drag_target(start, pointer_start, Vec2::new(-2_000.0, -2_000.0), bounds);
        assert_eq!(clamped_low, Vec2::ZERO);

        let clamped_high = drag_target(start, pointer_start, Vec2::new(5_000.0, 5_000.0), bounds);
        assert_eq!(clamped_high, Vec2::new(1_160.0, 680.0));
    }

    #[test]
    fn initial_window_fit_keeps_the_entire_panel_inside_720p() {
        let bounds = Vec2::new(1280.0, 720.0);
        assert_eq!(
            fit_window_origin(Vec2::new(1120.0, 430.0), Vec2::new(330.0, 320.0), bounds,),
            Vec2::new(950.0, 400.0),
        );
        assert_eq!(
            fit_window_origin(Vec2::new(-20.0, -5.0), Vec2::new(300.0, 200.0), bounds),
            Vec2::ZERO,
        );
    }

    #[test]
    fn minimize_button_collapses_content_and_restores_it() {
        let mut app = App::new();
        app.add_plugins(ToolWindowsPlugin);
        let mut window_entity = Entity::PLACEHOLDER;
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            commands
                .spawn(Node {
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|parent| {
                    window_entity = spawn_tool_window(
                        parent,
                        "TEST WINDOW",
                        Vec2::new(20.0, 30.0),
                        ToolWindowStyle::default(),
                        (),
                        |content| {
                            content.spawn(Text::new("body"));
                        },
                    );
                });
        }
        app.world_mut().flush();
        app.update();

        let minimize_button = app
            .world_mut()
            .query::<(Entity, &ToolWindowMinimizeButton)>()
            .iter(app.world())
            .find(|(_, button)| button.window == window_entity)
            .map(|(entity, _)| entity)
            .expect("minimize button exists");
        let content_entity = app
            .world_mut()
            .query::<(Entity, &ToolWindowContent)>()
            .iter(app.world())
            .find(|(_, content)| content.window == window_entity)
            .map(|(entity, _)| entity)
            .expect("content exists");

        let press = |app: &mut App| {
            *app.world_mut()
                .get_mut::<Interaction>(minimize_button)
                .unwrap() = Interaction::Pressed;
            app.update();
            *app.world_mut()
                .get_mut::<Interaction>(minimize_button)
                .unwrap() = Interaction::None;
            app.update();
        };

        press(&mut app);
        assert!(
            app.world()
                .get::<ToolWindow>(window_entity)
                .unwrap()
                .minimized
        );
        assert_eq!(
            app.world().get::<Node>(content_entity).unwrap().display,
            Display::None
        );

        press(&mut app);
        assert!(
            !app.world()
                .get::<ToolWindow>(window_entity)
                .unwrap()
                .minimized
        );
        assert_eq!(
            app.world().get::<Node>(content_entity).unwrap().display,
            Display::Flex
        );
    }
}
