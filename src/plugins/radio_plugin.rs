//! Radio chatter — diegetic narrative HUD. Lower-third scrolling list of speaker
//! lines color-coded by faction. Driven by `RadioChatterEvent`.

use bevy::prelude::*;

use crate::events::RadioChatterEvent;
use crate::resources::{RadioChatter, RadioLine};

pub struct RadioPlugin;

impl Plugin for RadioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RadioChatter>()
            .add_systems(Startup, setup_radio_panel)
            .add_systems(
                Update,
                (ingest_radio_events, tick_radio_lines, render_radio_panel),
            );
    }
}

#[derive(Component)]
struct RadioRoot;
#[derive(Component)]
struct RadioText(usize); // line index

const MAX_LINES: usize = 4;

fn setup_radio_panel(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(15.0),
                bottom: Val::Px(120.0),
                width: Val::Percent(70.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            RadioRoot,
        ))
        .with_children(|p| {
            for i in 0..MAX_LINES {
                p.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                    RadioText(i),
                ));
            }
        });
}

fn ingest_radio_events(
    mut events: MessageReader<RadioChatterEvent>,
    mut chatter: ResMut<RadioChatter>,
) {
    for ev in events.read() {
        chatter.lines.push(RadioLine {
            speaker: ev.speaker.clone(),
            text: ev.text.clone(),
            color: ev.faction.dialogue_color(),
            remaining: ev.duration,
        });
        if chatter.lines.len() > MAX_LINES {
            let drop = chatter.lines.len() - MAX_LINES;
            chatter.lines.drain(0..drop);
        }
    }
}

fn tick_radio_lines(time: Res<Time>, mut chatter: ResMut<RadioChatter>) {
    let dt = time.delta_secs();
    for line in chatter.lines.iter_mut() {
        line.remaining = (line.remaining - dt).max(0.0);
    }
    chatter.lines.retain(|l| l.remaining > 0.0);
}

fn render_radio_panel(
    chatter: Res<RadioChatter>,
    mut q: Query<(&RadioText, &mut Text, &mut TextColor)>,
) {
    for (slot, mut text, mut color) in q.iter_mut() {
        if let Some(line) = chatter.lines.get(slot.0) {
            text.0 = format!("[{}] {}", line.speaker, line.text);
            let fade = (line.remaining * 1.5).clamp(0.0, 1.0);
            let c = line.color.to_srgba();
            color.0 = Color::srgba(c.red, c.green, c.blue, fade);
        } else {
            text.0.clear();
            color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
        }
    }
}
