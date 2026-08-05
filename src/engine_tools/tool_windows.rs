//! Reusable movable, resizable, minimizable tool windows for creator screens.
//!
//! A tool window is an absolutely-positioned panel with a drag handle (its
//! title bar), a minimize/restore button, a corner resize grip, and a
//! scrollable content container. The interaction model follows professional
//! DCC tools (Blender is the reference): drag the header to move, drag the
//! ◢ grip to resize, double-click the header to collapse, and everything
//! stays reachable when the application window itself is resized — panels
//! larger than the new viewport shrink to fit rather than hanging off-screen.
//!
//! The framework is screen-agnostic: the Starfall Forge level workspace and
//! the Creature/Weapon Forges all build their panels through
//! [`spawn_tool_window`], and any future creator screen can join by spawning
//! windows under its own UI root.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Window chrome state. Lives on the window root node.
#[derive(Component, Debug, Default)]
pub struct ToolWindow {
    pub minimized: bool,
    drag: Option<DragState>,
    resize: Option<ResizeState>,
    /// Current full size (chrome + content), kept live through resizes so the
    /// fit system always clamps against the real footprint.
    requested_size: Vec2,
    /// Seconds-since-startup of the last title-bar press, for double-click
    /// collapse detection.
    last_title_press: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
struct DragState {
    pointer_start: Vec2,
    window_start: Vec2,
}

#[derive(Debug, Clone, Copy)]
struct ResizeState {
    pointer_start: Vec2,
    size_start: Vec2,
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

/// The corner resize grip of one window.
#[derive(Component, Debug, Clone, Copy)]
pub struct ToolWindowResizeGrip {
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
                tool_window_resize_system,
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

/// Smallest useful panel: enough for a header plus a couple of rows.
const MIN_WINDOW_SIZE: Vec2 = Vec2::new(200.0, 140.0);
/// Height of the title bar, the part of a window that is not content.
const TITLE_BAR_HEIGHT: f32 = 36.0;
/// Two presses of the title bar within this window collapse the panel.
const DOUBLE_CLICK_SECONDS: f32 = 0.35;

/// Where a corner-grip resize lands for a given pointer travel, clamped
/// between the minimum panel size and the viewport.
fn resize_target(size_start: Vec2, pointer_start: Vec2, pointer: Vec2, bounds: Vec2) -> Vec2 {
    let target = size_start + (pointer - pointer_start);
    clamp_size_to_bounds(target, bounds)
}

/// A window may never be smaller than the minimum or larger than the
/// viewport it lives in.
fn clamp_size_to_bounds(size: Vec2, bounds: Vec2) -> Vec2 {
    Vec2::new(
        size.x
            .clamp(MIN_WINDOW_SIZE.x, bounds.x.max(MIN_WINDOW_SIZE.x)),
        size.y
            .clamp(MIN_WINDOW_SIZE.y, bounds.y.max(MIN_WINDOW_SIZE.y)),
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
    time: Res<Time>,
    mut raise_order: ResMut<ToolWindowRaiseOrder>,
    primary: Query<&Window, With<PrimaryWindow>>,
    bars: Query<(&Interaction, &ToolWindowTitleBar), With<Button>>,
    mut windows: Query<(&mut ToolWindow, &mut Node)>,
    mut contents: Query<(&mut Node, &ToolWindowContent), Without<ToolWindow>>,
    mut glyphs: Query<(&mut Text, &ToolWindowMinimizeGlyph)>,
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
                // Blender-style header double-click: two grabs inside the
                // window collapse or restore the panel.
                let now = time.elapsed_secs();
                let double_click = window
                    .last_title_press
                    .is_some_and(|last| now - last <= DOUBLE_CLICK_SECONDS);
                window.last_title_press = Some(now);
                if double_click {
                    window.minimized = !window.minimized;
                    set_window_minimized(bar.window, window.minimized, &mut contents, &mut glyphs);
                }
            }
            Some(drag) => {
                let target = drag_target(drag.window_start, drag.pointer_start, cursor, bounds);
                node.left = Val::Px(target.x);
                node.top = Val::Px(target.y);
            }
        }
    }
}

/// Corner-grip resize. Dragging the ◢ grip changes the window's width and
/// its content height together, clamped between the minimum panel size and
/// the viewport, and keeps `requested_size` live so the fit system clamps
/// against the window's real footprint.
fn tool_window_resize_system(
    mut commands: Commands,
    mut raise_order: ResMut<ToolWindowRaiseOrder>,
    primary: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    grips: Query<(&Interaction, &ToolWindowResizeGrip), With<Button>>,
    mut windows: Query<(&mut ToolWindow, &mut Node)>,
    mut contents: Query<(&mut Node, &ToolWindowContent), Without<ToolWindow>>,
) {
    let Ok(primary) = primary.single() else {
        return;
    };
    let scale = ui_scale.as_deref().map_or(1.0, |scale| scale.0).max(0.01);
    let cursor = primary.cursor_position().map(|cursor| cursor / scale);
    let bounds = Vec2::new(primary.width(), primary.height()) / scale;
    for (interaction, grip) in &grips {
        let Ok((mut window, mut node)) = windows.get_mut(grip.window) else {
            continue;
        };
        if *interaction != Interaction::Pressed {
            if window.resize.is_some() {
                window.resize = None;
            }
            continue;
        }
        let Some(cursor) = cursor else {
            continue;
        };
        match window.resize {
            None => {
                window.resize = Some(ResizeState {
                    pointer_start: cursor,
                    size_start: window.requested_size,
                });
                raise_order.0 += 1;
                commands.entity(grip.window).insert(ZIndex(raise_order.0));
            }
            Some(resize) => {
                let target = resize_target(resize.size_start, resize.pointer_start, cursor, bounds);
                window.requested_size = target;
                apply_window_size(grip.window, target, &mut node, &mut contents);
            }
        }
    }
}

/// Write a footprint onto the window root and its content node. The content
/// carries the height (footprint minus the title bar) so scrolling keeps
/// working at every size.
fn apply_window_size(
    window_entity: Entity,
    size: Vec2,
    root_node: &mut Node,
    contents: &mut Query<(&mut Node, &ToolWindowContent), Without<ToolWindow>>,
) {
    root_node.width = Val::Px(size.x);
    for (mut content_node, content) in contents.iter_mut() {
        if content.window == window_entity {
            content_node.height = Val::Px((size.y - TITLE_BAR_HEIGHT).max(0.0));
        }
    }
}

fn tool_window_fit_system(
    primary: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    mut windows: Query<(Entity, &mut ToolWindow, &mut Node)>,
    mut contents: Query<(&mut Node, &ToolWindowContent), Without<ToolWindow>>,
) {
    let Ok(primary) = primary.single() else {
        return;
    };
    let scale = ui_scale.as_deref().map_or(1.0, |scale| scale.0).max(0.01);
    let logical_bounds = Vec2::new(primary.width(), primary.height()) / scale;
    for (entity, mut window, mut node) in windows.iter_mut() {
        if window.requested_size == Vec2::ZERO {
            continue;
        }
        // A panel larger than the (possibly just-shrunk) application window
        // shrinks to fit instead of hanging unreachable off-screen.
        let clamped = clamp_size_to_bounds(window.requested_size, logical_bounds);
        if clamped != window.requested_size {
            window.requested_size = clamped;
            apply_window_size(entity, clamped, &mut node, &mut contents);
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
    mut contents: Query<(&mut Node, &ToolWindowContent), Without<ToolWindow>>,
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
        set_window_minimized(button.window, window.minimized, &mut contents, &mut glyphs);
    }
}

/// Collapse or restore a window's content and swap the header glyph. Shared
/// by the minimize button and the header double-click.
fn set_window_minimized(
    window_entity: Entity,
    minimized: bool,
    contents: &mut Query<(&mut Node, &ToolWindowContent), Without<ToolWindow>>,
    glyphs: &mut Query<(&mut Text, &ToolWindowMinimizeGlyph)>,
) {
    for (mut node, content) in contents.iter_mut() {
        if content.window == window_entity {
            node.display = if minimized {
                Display::None
            } else {
                Display::Flex
            };
        }
    }
    for (mut text, glyph) in glyphs.iter_mut() {
        if glyph.window == window_entity {
            *text = Text::new(if minimized { "▣" } else { "—" });
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
            // Corner resize grip, floated over the bottom-right so it never
            // participates in the column layout.
            window
                .spawn((
                    Button,
                    ToolWindowResizeGrip {
                        window: window_entity,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        right: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        min_width: Val::Px(22.0),
                        min_height: Val::Px(22.0),
                        align_items: AlignItems::FlexEnd,
                        justify_content: JustifyContent::FlexEnd,
                        padding: UiRect::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
                ))
                .with_children(|grip| {
                    grip.spawn((
                        Text::new("◢"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(style.accent.with_alpha(0.85)),
                    ));
                });
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
    fn resize_target_clamps_between_minimum_and_viewport() {
        let bounds = Vec2::new(1280.0, 720.0);
        let start = Vec2::new(300.0, 400.0);
        let pointer_start = Vec2::new(310.0, 430.0);

        // Dragging outward grows the window by the pointer travel.
        let grown = resize_target(start, pointer_start, Vec2::new(410.0, 480.0), bounds);
        assert_eq!(grown, Vec2::new(400.0, 450.0));

        // Dragging far inward stops at the minimum useful panel…
        let tiny = resize_target(start, pointer_start, Vec2::new(-900.0, -900.0), bounds);
        assert_eq!(tiny, MIN_WINDOW_SIZE);

        // …and dragging past the screen edge stops at the viewport.
        let huge = resize_target(start, pointer_start, Vec2::new(9_000.0, 9_000.0), bounds);
        assert_eq!(huge, bounds);
    }

    #[test]
    fn a_shrunken_app_window_shrinks_oversized_panels_to_fit() {
        // 4K-sized panel, laptop-sized window: the panel must fit, and a
        // degenerate viewport must never push a panel below the minimum.
        let clamped = clamp_size_to_bounds(Vec2::new(2_000.0, 1_400.0), Vec2::new(1_280.0, 720.0));
        assert_eq!(clamped, Vec2::new(1_280.0, 720.0));
        let degenerate = clamp_size_to_bounds(Vec2::new(300.0, 300.0), Vec2::new(50.0, 40.0));
        assert_eq!(degenerate, MIN_WINDOW_SIZE);
    }

    /// Harness with a synthetic primary window so cursor-driven systems run
    /// headlessly.
    fn windowed_app() -> App {
        let mut app = App::new();
        app.add_plugins(ToolWindowsPlugin);
        app.init_resource::<Time>();
        app.world_mut().spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(1280, 720),
                ..default()
            },
            PrimaryWindow,
        ));
        app
    }

    fn set_cursor(app: &mut App, position: Vec2) {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).unwrap();
        window.set_cursor_position(Some(position));
    }

    #[test]
    fn dragging_the_grip_resizes_the_window_and_its_content() {
        let mut app = windowed_app();
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
                        "RESIZE ME",
                        Vec2::new(40.0, 40.0),
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

        let grip = app
            .world_mut()
            .query::<(Entity, &ToolWindowResizeGrip)>()
            .iter(app.world())
            .find(|(_, grip)| grip.window == window_entity)
            .map(|(entity, _)| entity)
            .expect("grip exists");
        let content = app
            .world_mut()
            .query::<(Entity, &ToolWindowContent)>()
            .iter(app.world())
            .find(|(_, content)| content.window == window_entity)
            .map(|(entity, _)| entity)
            .expect("content exists");

        let size_before = app
            .world()
            .get::<ToolWindow>(window_entity)
            .unwrap()
            .requested_size;

        // Press the grip, then pull it 60px right and 40px down.
        set_cursor(&mut app, Vec2::new(320.0, 600.0));
        *app.world_mut().get_mut::<Interaction>(grip).unwrap() = Interaction::Pressed;
        app.update();
        set_cursor(&mut app, Vec2::new(380.0, 640.0));
        app.update();

        let window = app.world().get::<ToolWindow>(window_entity).unwrap();
        assert_eq!(
            window.requested_size,
            size_before + Vec2::new(60.0, 40.0),
            "the window grows by exactly the pointer travel"
        );
        let root = app.world().get::<Node>(window_entity).unwrap();
        assert_eq!(root.width, Val::Px(size_before.x + 60.0));
        let content_node = app.world().get::<Node>(content).unwrap();
        assert_eq!(
            content_node.height,
            Val::Px(size_before.y + 40.0 - TITLE_BAR_HEIGHT),
            "content keeps everything except the title bar"
        );

        // Releasing ends the gesture: further cursor movement changes nothing.
        *app.world_mut().get_mut::<Interaction>(grip).unwrap() = Interaction::None;
        app.update();
        set_cursor(&mut app, Vec2::new(900.0, 700.0));
        app.update();
        let window = app.world().get::<ToolWindow>(window_entity).unwrap();
        assert_eq!(window.requested_size, size_before + Vec2::new(60.0, 40.0));
    }

    #[test]
    fn minimize_button_collapses_content_and_restores_it() {
        let mut app = windowed_app();
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
