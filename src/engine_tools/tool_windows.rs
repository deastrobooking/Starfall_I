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

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::ui::{ComputedStackIndex, FocusPolicy, RelativeCursorPosition, UiSystems};
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

/// Marks pointer-only window chrome that should not participate in generic
/// keyboard/controller menu focus. The minimize button deliberately does not
/// carry this marker: it is a meaningful, focusable window action.
#[derive(Component, Debug, Clone, Copy)]
pub struct ToolWindowChromeControl;

/// Text glyph on the minimize button ("—" open, "▣" minimized).
#[derive(Component, Debug, Clone, Copy)]
struct ToolWindowMinimizeGlyph {
    window: Entity,
}

/// Monotonic sibling z-order: grabbing a window raises it above the others.
#[derive(Resource, Default)]
struct ToolWindowRaiseOrder(i32);

/// Current pointer ownership for creator windows. Viewport tools should run
/// after [`ToolWindowSystemSet::PointerState`] and skip pointer-driven actions
/// while this resource reports capture.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ToolWindowPointerState {
    captures_viewport: bool,
}

impl ToolWindowPointerState {
    pub fn captures_viewport(&self) -> bool {
        self.captures_viewport
    }
}

/// Public ordering boundary for viewport/editor systems that share pointer
/// input with floating creator windows.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolWindowSystemSet {
    /// Pointer hover/gesture ownership has been refreshed for this frame.
    PointerState,
    Interaction,
    FitAndState,
}

pub struct ToolWindowsPlugin;

impl Plugin for ToolWindowsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ToolWindowRaiseOrder>()
            .init_resource::<ToolWindowPointerState>()
            .configure_sets(
                Update,
                (
                    ToolWindowSystemSet::PointerState,
                    ToolWindowSystemSet::Interaction,
                    ToolWindowSystemSet::FitAndState,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                tool_window_pointer_state_system.in_set(ToolWindowSystemSet::PointerState),
            )
            .add_systems(
                Update,
                (
                    tool_window_raise_system,
                    tool_window_drag_system,
                    tool_window_resize_system,
                    tool_window_scroll_system,
                    tool_window_minimize_system,
                )
                    .chain()
                    .in_set(ToolWindowSystemSet::Interaction),
            )
            .add_systems(
                Update,
                tool_window_fit_system.in_set(ToolWindowSystemSet::FitAndState),
            );
        // Relative cursor positions and stack indices are finalized after UI
        // layout. Refresh capture again there so consumers in the next Update
        // begin from actual, current geometry rather than spawn defaults.
        app.add_systems(
            PostUpdate,
            tool_window_pointer_state_system.after(UiSystems::Stack),
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

fn tool_window_pointer_state_system(
    mut pointer_state: ResMut<ToolWindowPointerState>,
    windows: Query<(&RelativeCursorPosition, &ToolWindow)>,
) {
    pointer_state.captures_viewport = windows.iter().any(|(cursor, window)| {
        cursor.cursor_over() || window.drag.is_some() || window.resize.is_some()
    });
}

/// Raise a window when any interactive surface inside it is pressed, including
/// ordinary content controls. Header/grip gesture systems do not own z-order,
/// avoiding duplicate increments on a single press.
fn tool_window_raise_system(
    mut commands: Commands,
    mut raise_order: ResMut<ToolWindowRaiseOrder>,
    direct_surfaces: Query<
        (
            &Interaction,
            Option<&ToolWindowTitleBar>,
            Option<&ToolWindowMinimizeButton>,
            Option<&ToolWindowResizeGrip>,
            Option<&ToolWindowContent>,
        ),
        (
            Changed<Interaction>,
            Or<(
                With<ToolWindowTitleBar>,
                With<ToolWindowMinimizeButton>,
                With<ToolWindowResizeGrip>,
                With<ToolWindowContent>,
            )>,
        ),
    >,
    content_buttons: Query<
        (&Interaction, &ChildOf),
        (
            With<Button>,
            Changed<Interaction>,
            Without<ToolWindowTitleBar>,
            Without<ToolWindowMinimizeButton>,
            Without<ToolWindowResizeGrip>,
            Without<ToolWindowContent>,
        ),
    >,
    parents: Query<&ChildOf>,
    windows: Query<(), With<ToolWindow>>,
) {
    let mut pressed_windows = Vec::new();
    pressed_windows.extend(direct_surfaces.iter().filter_map(
        |(interaction, title, minimize, grip, content)| {
            if *interaction != Interaction::Pressed {
                return None;
            }
            title
                .map(|marker| marker.window)
                .or_else(|| minimize.map(|marker| marker.window))
                .or_else(|| grip.map(|marker| marker.window))
                .or_else(|| content.map(|marker| marker.window))
        },
    ));
    pressed_windows.extend(content_buttons.iter().filter_map(|(interaction, parent)| {
        if *interaction != Interaction::Pressed {
            return None;
        }
        std::iter::successors(Some(parent.parent()), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .take(64)
        .find(|entity| windows.contains(*entity))
    }));
    pressed_windows.sort_unstable();
    pressed_windows.dedup();

    for window in pressed_windows {
        raise_order.0 = raise_order.0.saturating_add(1);
        commands.entity(window).insert(ZIndex(raise_order.0));
    }
}

fn tool_window_drag_system(
    time: Res<Time>,
    primary: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    bars: Query<(&Interaction, &ToolWindowTitleBar), With<Button>>,
    mut windows: Query<(&mut ToolWindow, &mut Node)>,
    mut contents: Query<
        (&mut Node, &ToolWindowContent),
        (Without<ToolWindow>, Without<ToolWindowResizeGrip>),
    >,
    mut glyphs: Query<(&mut Text, &ToolWindowMinimizeGlyph)>,
    mut grips: Query<
        (&mut Node, &ToolWindowResizeGrip),
        (Without<ToolWindow>, Without<ToolWindowContent>),
    >,
) {
    let Ok(primary) = primary.single() else {
        return;
    };
    let scale = ui_scale.as_deref().map_or(1.0, |scale| scale.0).max(0.01);
    let cursor = primary.cursor_position().map(|cursor| cursor / scale);
    let bounds = Vec2::new(primary.width(), primary.height()) / scale;
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
                // Blender-style header double-click: two grabs inside the
                // window collapse or restore the panel.
                let now = time.elapsed_secs();
                let double_click = window
                    .last_title_press
                    .is_some_and(|last| now - last <= DOUBLE_CLICK_SECONDS);
                window.last_title_press = Some(now);
                if double_click {
                    window.minimized = !window.minimized;
                    set_window_minimized(
                        bar.window,
                        window.minimized,
                        &mut contents,
                        &mut glyphs,
                        &mut grips,
                    );
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
    primary: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    grips: Query<(&Interaction, &ToolWindowResizeGrip), With<Button>>,
    mut windows: Query<(&mut ToolWindow, &mut Node)>,
    mut contents: Query<
        (&mut Node, &ToolWindowContent),
        (Without<ToolWindow>, Without<ToolWindowResizeGrip>),
    >,
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
            }
            Some(resize) => {
                let target = resize_target(resize.size_start, resize.pointer_start, cursor, bounds);
                window.requested_size = target;
                apply_window_size(grip.window, target, &mut node, &mut contents);
            }
        }
    }
}

const TOOL_WINDOW_SCROLL_LINE: f32 = 26.0;

fn tool_window_scroll_system(
    mut wheel: MessageReader<MouseWheel>,
    mut contents: ParamSet<(
        Query<
            (
                Entity,
                &RelativeCursorPosition,
                &ComputedStackIndex,
                &ComputedNode,
                &Node,
            ),
            With<ToolWindowContent>,
        >,
        Query<(&mut ScrollPosition, &ComputedNode), With<ToolWindowContent>>,
    )>,
) {
    let mut delta = 0.0;
    for event in wheel.read() {
        let scale = match event.unit {
            MouseScrollUnit::Line => TOOL_WINDOW_SCROLL_LINE,
            MouseScrollUnit::Pixel => 1.0,
        };
        delta -= event.y * scale;
    }
    if delta == 0.0 {
        return;
    }

    let topmost = contents
        .p0()
        .iter()
        .filter(|(_, cursor, _, computed, node)| {
            cursor.cursor_over() && node.display != Display::None && !computed.is_empty()
        })
        .max_by_key(|(_, _, stack, _, _)| stack.0)
        .map(|(entity, _, _, _, _)| entity);
    let Some(topmost) = topmost else {
        return;
    };

    if let Ok((mut scroll, computed)) = contents.p1().get_mut(topmost) {
        let max_y = ((computed.content_size().y - computed.size().y)
            * computed.inverse_scale_factor())
        .max(0.0);
        scroll.y = (scroll.y + delta).clamp(0.0, max_y);
    }
}

/// Write a footprint onto the window root and its content node. The content
/// carries the height (footprint minus the title bar) so scrolling keeps
/// working at every size.
fn apply_window_size(
    window_entity: Entity,
    size: Vec2,
    root_node: &mut Node,
    contents: &mut Query<
        (&mut Node, &ToolWindowContent),
        (Without<ToolWindow>, Without<ToolWindowResizeGrip>),
    >,
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
    parents: Query<&ChildOf>,
    parent_nodes: Query<&ComputedNode, Without<ToolWindow>>,
    mut windows: Query<(Entity, &mut ToolWindow, &mut Node)>,
    mut contents: Query<
        (&mut Node, &ToolWindowContent),
        (Without<ToolWindow>, Without<ToolWindowResizeGrip>),
    >,
) {
    let Ok(primary) = primary.single() else {
        return;
    };
    let scale = ui_scale.as_deref().map_or(1.0, |scale| scale.0).max(0.01);
    let primary_bounds = Vec2::new(primary.width(), primary.height()) / scale;
    for (entity, mut window, mut node) in windows.iter_mut() {
        if window.requested_size == Vec2::ZERO {
            continue;
        }
        // A panel larger than the (possibly just-shrunk) application window
        // shrinks to fit instead of hanging unreachable off-screen.
        let logical_bounds =
            tool_window_parent_bounds(entity, primary_bounds, &parents, &parent_nodes);
        let clamped = clamp_size_to_bounds(window.requested_size, logical_bounds);
        if clamped != window.requested_size {
            window.requested_size = clamped;
            apply_window_size(entity, clamped, &mut node, &mut contents);
        }
        let current = Vec2::new(px_or_zero(node.left), px_or_zero(node.top));
        let footprint = if window.minimized {
            Vec2::new(window.requested_size.x, TITLE_BAR_HEIGHT)
        } else {
            window.requested_size
        };
        let fitted = fit_window_origin(current, footprint, logical_bounds);
        node.left = Val::Px(fitted.x);
        node.top = Val::Px(fitted.y);
    }
}

fn tool_window_parent_bounds(
    entity: Entity,
    fallback: Vec2,
    parents: &Query<&ChildOf>,
    nodes: &Query<&ComputedNode, Without<ToolWindow>>,
) -> Vec2 {
    let Ok(parent) = parents.get(entity) else {
        return fallback;
    };
    let Ok(computed) = nodes.get(parent.parent()) else {
        return fallback;
    };
    logical_node_size(computed, fallback)
}

fn logical_node_size(computed: &ComputedNode, fallback: Vec2) -> Vec2 {
    if computed.is_empty() {
        fallback
    } else {
        computed.size() * computed.inverse_scale_factor()
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
    mut contents: Query<
        (&mut Node, &ToolWindowContent),
        (Without<ToolWindow>, Without<ToolWindowResizeGrip>),
    >,
    mut glyphs: Query<(&mut Text, &ToolWindowMinimizeGlyph)>,
    mut grips: Query<
        (&mut Node, &ToolWindowResizeGrip),
        (Without<ToolWindow>, Without<ToolWindowContent>),
    >,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Ok(mut window) = windows.get_mut(button.window) else {
            continue;
        };
        window.minimized = !window.minimized;
        set_window_minimized(
            button.window,
            window.minimized,
            &mut contents,
            &mut glyphs,
            &mut grips,
        );
    }
}

/// Collapse or restore a window's content and swap the header glyph. Shared
/// by the minimize button and the header double-click.
fn set_window_minimized(
    window_entity: Entity,
    minimized: bool,
    contents: &mut Query<
        (&mut Node, &ToolWindowContent),
        (Without<ToolWindow>, Without<ToolWindowResizeGrip>),
    >,
    glyphs: &mut Query<(&mut Text, &ToolWindowMinimizeGlyph)>,
    grips: &mut Query<
        (&mut Node, &ToolWindowResizeGrip),
        (Without<ToolWindow>, Without<ToolWindowContent>),
    >,
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
    for (mut node, grip) in grips.iter_mut() {
        if grip.window == window_entity {
            node.display = if minimized {
                Display::None
            } else {
                Display::Flex
            };
        }
    }
}

/// Visual defaults shared by every tool window.
pub struct ToolWindowStyle {
    pub accent: Color,
    pub background: Color,
    pub width: f32,
    pub content_height: Val,
    /// Spawn with only the title bar visible. The stored full size is retained
    /// so restoring returns to the requested dimensions.
    pub initially_minimized: bool,
}

impl Default for ToolWindowStyle {
    fn default() -> Self {
        Self {
            accent: Color::srgb(0.14, 0.42, 0.62),
            background: Color::srgba(0.035, 0.05, 0.085, 0.94),
            width: 286.0,
            content_height: Val::Px(540.0),
            initially_minimized: false,
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
    let initially_minimized = style.initially_minimized;
    parent
        .spawn((
            ToolWindow {
                minimized: initially_minimized,
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
            RelativeCursorPosition::default(),
        ))
        .with_children(|window| {
            window_entity = window.target_entity();
            window
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(TITLE_BAR_HEIGHT),
                        align_items: AlignItems::Stretch,
                        ..default()
                    },
                    BackgroundColor(style.accent.with_alpha(0.35)),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Button,
                        ToolWindowChromeControl,
                        ToolWindowTitleBar {
                            window: window_entity,
                        },
                        Node {
                            flex_grow: 1.0,
                            min_height: Val::Px(TITLE_BAR_HEIGHT),
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|drag_handle| {
                        drag_handle.spawn((
                            Text::new(title),
                            TextFont {
                                font_size: FontSize::Px(16.0),
                                ..default()
                            },
                            TextColor(Color::srgb(1.0, 0.78, 0.25)),
                        ));
                    });
                    bar.spawn((
                        Button,
                        ToolWindowMinimizeButton {
                            window: window_entity,
                        },
                        Node {
                            min_width: Val::Px(36.0),
                            min_height: Val::Px(36.0),
                            border: UiRect::all(Val::Px(1.0)),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
                        BorderColor::all(style.accent.with_alpha(0.8)),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new(if initially_minimized { "▣" } else { "—" }),
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
                    Interaction::default(),
                    FocusPolicy::Block,
                    RelativeCursorPosition::default(),
                    Node {
                        display: if initially_minimized {
                            Display::None
                        } else {
                            Display::Flex
                        },
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
                    ToolWindowChromeControl,
                    ToolWindowResizeGrip {
                        window: window_entity,
                    },
                    Node {
                        display: if initially_minimized {
                            Display::None
                        } else {
                            Display::Flex
                        },
                        position_type: PositionType::Absolute,
                        right: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        min_width: Val::Px(36.0),
                        min_height: Val::Px(36.0),
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
        app.add_message::<MouseWheel>();
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
        let grip_entity = app
            .world_mut()
            .query::<(Entity, &ToolWindowResizeGrip)>()
            .iter(app.world())
            .find(|(_, grip)| grip.window == window_entity)
            .map(|(entity, _)| entity)
            .expect("resize grip exists");

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
        assert_eq!(
            app.world().get::<Node>(grip_entity).unwrap().display,
            Display::None,
            "the resize grip must not cover the 36px restore target"
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
        assert_eq!(
            app.world().get::<Node>(grip_entity).unwrap().display,
            Display::Flex
        );
    }

    #[test]
    fn dragging_uses_logical_coordinates_at_non_default_ui_scale() {
        let mut app = windowed_app();
        app.insert_resource(UiScale(2.0));
        let mut window_entity = Entity::PLACEHOLDER;
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            commands.spawn(Node::default()).with_children(|parent| {
                window_entity = spawn_tool_window(
                    parent,
                    "SCALED DRAG",
                    Vec2::new(40.0, 40.0),
                    ToolWindowStyle {
                        content_height: Val::Px(120.0),
                        ..default()
                    },
                    (),
                    |_| {},
                );
            });
        }
        app.world_mut().flush();
        app.update();

        let title = app
            .world_mut()
            .query::<(Entity, &ToolWindowTitleBar)>()
            .iter(app.world())
            .find(|(_, title)| title.window == window_entity)
            .map(|(entity, _)| entity)
            .unwrap();
        set_cursor(&mut app, Vec2::new(100.0, 100.0));
        *app.world_mut().get_mut::<Interaction>(title).unwrap() = Interaction::Pressed;
        app.update();
        set_cursor(&mut app, Vec2::new(200.0, 140.0));
        app.update();

        let node = app.world().get::<Node>(window_entity).unwrap();
        assert_eq!(node.left, Val::Px(90.0));
        assert_eq!(node.top, Val::Px(60.0));
    }

    #[test]
    fn wheel_scroll_routes_to_topmost_hovered_content_and_clamps() {
        use bevy::input::touch::TouchPhase;

        let mut app = App::new();
        app.add_message::<MouseWheel>();
        app.add_plugins(ToolWindowsPlugin);
        app.init_resource::<Time>();

        let spawn_content = |world: &mut World, stack: u32, scroll_y: f32| {
            world
                .spawn((
                    ToolWindowContent {
                        window: Entity::PLACEHOLDER,
                    },
                    RelativeCursorPosition {
                        cursor_over: true,
                        normalized: Some(Vec2::ZERO),
                    },
                    ComputedStackIndex(stack),
                    ComputedNode {
                        size: Vec2::new(100.0, 100.0),
                        content_size: Vec2::new(100.0, 200.0),
                        ..default()
                    },
                    Node::default(),
                    ScrollPosition(Vec2::new(0.0, scroll_y)),
                ))
                .id()
        };
        let lower = spawn_content(app.world_mut(), 2, 0.0);
        let upper = spawn_content(app.world_mut(), 9, 90.0);
        app.world_mut().write_message(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: -1.0,
            window: Entity::PLACEHOLDER,
            phase: TouchPhase::Moved,
        });
        app.update();

        assert_eq!(app.world().get::<ScrollPosition>(lower).unwrap().y, 0.0);
        assert_eq!(
            app.world().get::<ScrollPosition>(upper).unwrap().y,
            100.0,
            "the top surface receives the wheel and clamps at its scroll extent"
        );
    }

    #[test]
    fn pointer_capture_tracks_hover_and_an_owned_drag() {
        let mut app = App::new();
        app.add_message::<MouseWheel>();
        app.add_plugins(ToolWindowsPlugin);
        app.init_resource::<Time>();
        let root = app
            .world_mut()
            .spawn((
                ToolWindow::default(),
                Node::default(),
                RelativeCursorPosition {
                    cursor_over: true,
                    normalized: Some(Vec2::ZERO),
                },
            ))
            .id();
        app.update();
        assert!(app
            .world()
            .resource::<ToolWindowPointerState>()
            .captures_viewport());

        app.world_mut()
            .get_mut::<RelativeCursorPosition>(root)
            .unwrap()
            .cursor_over = false;
        app.world_mut().get_mut::<ToolWindow>(root).unwrap().drag = Some(DragState {
            pointer_start: Vec2::ZERO,
            window_start: Vec2::ZERO,
        });
        app.update();
        assert!(app
            .world()
            .resource::<ToolWindowPointerState>()
            .captures_viewport());

        app.world_mut().get_mut::<ToolWindow>(root).unwrap().drag = None;
        app.update();
        assert!(!app
            .world()
            .resource::<ToolWindowPointerState>()
            .captures_viewport());
    }

    #[test]
    fn initially_minimized_window_fits_title_only_and_restores_grip() {
        let mut app = windowed_app();
        let mut window_entity = Entity::PLACEHOLDER;
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            commands.spawn(Node::default()).with_children(|parent| {
                window_entity = spawn_tool_window(
                    parent,
                    "START COLLAPSED",
                    Vec2::new(100.0, 700.0),
                    ToolWindowStyle {
                        width: 300.0,
                        content_height: Val::Px(400.0),
                        initially_minimized: true,
                        ..default()
                    },
                    (),
                    |_| {},
                );
            });
        }
        app.world_mut().flush();
        app.update();

        let minimize = app
            .world_mut()
            .query::<(Entity, &ToolWindowMinimizeButton)>()
            .iter(app.world())
            .find(|(_, button)| button.window == window_entity)
            .map(|(entity, _)| entity)
            .unwrap();
        let content = app
            .world_mut()
            .query::<(Entity, &ToolWindowContent)>()
            .iter(app.world())
            .find(|(_, content)| content.window == window_entity)
            .map(|(entity, _)| entity)
            .unwrap();
        let grip = app
            .world_mut()
            .query::<(Entity, &ToolWindowResizeGrip)>()
            .iter(app.world())
            .find(|(_, grip)| grip.window == window_entity)
            .map(|(entity, _)| entity)
            .unwrap();

        assert_eq!(
            app.world().get::<Node>(window_entity).unwrap().top,
            Val::Px(684.0),
            "collapsed fitting uses the 36px title footprint"
        );
        assert_eq!(
            app.world().get::<Node>(content).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(grip).unwrap().display,
            Display::None
        );

        *app.world_mut().get_mut::<Interaction>(minimize).unwrap() = Interaction::Pressed;
        app.update();
        assert_eq!(
            app.world().get::<Node>(content).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Node>(grip).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Node>(window_entity).unwrap().top,
            Val::Px(284.0),
            "restoring refits the full 436px panel"
        );
    }

    #[test]
    fn fit_prefers_the_actual_parent_workspace_bounds() {
        let mut app = windowed_app();
        let mut window_entity = Entity::PLACEHOLDER;
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            commands
                .spawn((
                    Node::default(),
                    ComputedNode {
                        size: Vec2::new(400.0, 300.0),
                        content_size: Vec2::new(400.0, 300.0),
                        ..default()
                    },
                ))
                .with_children(|parent| {
                    window_entity = spawn_tool_window(
                        parent,
                        "WORKSPACE BOUNDS",
                        Vec2::new(350.0, 250.0),
                        ToolWindowStyle {
                            width: 300.0,
                            content_height: Val::Px(164.0),
                            ..default()
                        },
                        (),
                        |_| {},
                    );
                });
        }
        app.world_mut().flush();
        app.update();

        let node = app.world().get::<Node>(window_entity).unwrap();
        assert_eq!(node.left, Val::Px(100.0));
        assert_eq!(node.top, Val::Px(100.0));
    }

    #[test]
    fn parent_bounds_are_converted_from_physical_to_logical_units() {
        let computed = ComputedNode {
            size: Vec2::new(800.0, 500.0),
            inverse_scale_factor: 0.5,
            ..default()
        };
        assert_eq!(
            logical_node_size(&computed, Vec2::new(1_280.0, 720.0)),
            Vec2::new(400.0, 250.0)
        );
    }

    #[test]
    fn minimize_is_a_focusable_sibling_of_the_drag_handle() {
        let mut app = windowed_app();
        let mut window_entity = Entity::PLACEHOLDER;
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            commands.spawn(Node::default()).with_children(|parent| {
                window_entity = spawn_tool_window(
                    parent,
                    "SEMANTIC HEADER",
                    Vec2::ZERO,
                    ToolWindowStyle::default(),
                    (),
                    |_| {},
                );
            });
        }
        app.world_mut().flush();

        let title = app
            .world_mut()
            .query::<(Entity, &ToolWindowTitleBar)>()
            .iter(app.world())
            .find(|(_, marker)| marker.window == window_entity)
            .map(|(entity, _)| entity)
            .unwrap();
        let minimize = app
            .world_mut()
            .query::<(Entity, &ToolWindowMinimizeButton)>()
            .iter(app.world())
            .find(|(_, marker)| marker.window == window_entity)
            .map(|(entity, _)| entity)
            .unwrap();

        assert_eq!(
            app.world().get::<ChildOf>(title).unwrap().parent(),
            app.world().get::<ChildOf>(minimize).unwrap().parent(),
            "drag and minimize must be siblings, never nested buttons"
        );
        assert!(app.world().get::<ToolWindowChromeControl>(title).is_some());
        assert!(app
            .world()
            .get::<ToolWindowChromeControl>(minimize)
            .is_none());
        assert!(app.world().get::<BackgroundColor>(minimize).is_some());
        assert!(app.world().get::<BorderColor>(minimize).is_some());
        let node = app.world().get::<Node>(minimize).unwrap();
        assert_eq!(node.min_width, Val::Px(36.0));
        assert_eq!(node.min_height, Val::Px(36.0));
    }
}
