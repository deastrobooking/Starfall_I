#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
/// 3-D turtle graphics interpreter for L-system strings.
///
/// Symbol table:
///
/// ```text
///  F A B C   draw a branch segment and move forward
///  f          move forward (no segment drawn)
///  +          yaw  left   (+angle around local Up)
///  -          yaw  right  (-angle around local Up)
///  ^          pitch up    (+angle around local Right)
///  &          pitch down  (-angle around local Right)
///  \  (or l)  roll left   (+angle around local Forward)
///  /  (or r)  roll right  (-angle around local Forward)
///  |          reverse direction (180° yaw)
///  [          push state  (enters a branch; shrinks step & radius)
///  ]          pop state   (exits branch; places leaf cluster at tip)
///  L          place leaf cluster at current position
///  !          decrement radius by `radius_scale` factor
/// ```
///
/// All other characters are silently ignored (used as grammar variables).
use bevy::math::{Quat, Vec3};

// ── Output geometry ───────────────────────────────────────────────────────────

/// A single branch segment, ready to be turned into a cylinder entity.
#[derive(Debug, Clone)]
pub struct BranchSegment {
    /// Start point in tree-local space.
    pub start: Vec3,
    /// End point in tree-local space.
    pub end: Vec3,
    /// Radius of the cylinder at this segment.
    pub radius: f32,
    /// How many `[` levels deep this segment is (0 = trunk).
    pub depth: u32,
}

/// A leaf cluster to be rendered as a small sphere at a branch tip.
#[derive(Debug, Clone)]
pub struct LeafCluster {
    pub position: Vec3,
    /// Sphere radius for the cluster.
    pub size: f32,
    pub depth: u32,
}

/// Everything the turtle interpreter produces from one L-system string.
#[derive(Debug, Default, Clone)]
pub struct TurtleResult {
    pub branches: Vec<BranchSegment>,
    pub leaves: Vec<LeafCluster>,
}

// ── Internal turtle state ─────────────────────────────────────────────────────

#[derive(Clone)]
struct State {
    pos: Vec3,
    fwd: Vec3,   // local +Z in tree space (direction of growth)
    up: Vec3,    // local +Y
    right: Vec3, // local +X
    radius: f32,
    step: f32,
    depth: u32,
}

// ── Turtle ───────────────────────────────────────────────────────────────────

pub struct Turtle {
    angle: f32, // radians
    step: f32,
    length_scale: f32,
    start_radius: f32,
    radius_scale: f32,
}

impl Turtle {
    pub fn new(
        angle_deg: f32,
        step: f32,
        length_scale: f32,
        start_radius: f32,
        radius_scale: f32,
    ) -> Self {
        Self {
            angle: angle_deg.to_radians(),
            step,
            length_scale,
            start_radius,
            radius_scale,
        }
    }

    /// Interpret `string` and return all branch segments and leaf clusters.
    pub fn interpret(&self, string: &str) -> TurtleResult {
        let mut result = TurtleResult::default();
        let mut stack: Vec<State> = Vec::new();

        // Trees grow upward (+Y world axis).
        let mut state = State {
            pos: Vec3::ZERO,
            fwd: Vec3::Y,
            up: Vec3::Z,
            right: Vec3::X,
            radius: self.start_radius,
            step: self.step,
            depth: 0,
        };

        for ch in string.chars() {
            match ch {
                // ── Draw forward ──────────────────────────────────────────
                'F' | 'A' | 'B' | 'C' => {
                    let end = state.pos + state.fwd * state.step;
                    result.branches.push(BranchSegment {
                        start: state.pos,
                        end,
                        radius: state.radius,
                        depth: state.depth,
                    });
                    state.pos = end;
                }
                // ── Move without drawing ──────────────────────────────────
                'f' => {
                    state.pos += state.fwd * state.step;
                }
                // ── Yaw (rotate around local Up) ─────────────────────────
                '+' => {
                    let r = Quat::from_axis_angle(state.up, self.angle);
                    state.fwd = r * state.fwd;
                    state.right = r * state.right;
                }
                '-' => {
                    let r = Quat::from_axis_angle(state.up, -self.angle);
                    state.fwd = r * state.fwd;
                    state.right = r * state.right;
                }
                // ── Pitch (rotate around local Right) ─────────────────────
                '^' => {
                    let r = Quat::from_axis_angle(state.right, self.angle);
                    state.fwd = r * state.fwd;
                    state.up = r * state.up;
                }
                '&' => {
                    let r = Quat::from_axis_angle(state.right, -self.angle);
                    state.fwd = r * state.fwd;
                    state.up = r * state.up;
                }
                // ── Roll (rotate around local Forward) ────────────────────
                '\\' | 'l' => {
                    let r = Quat::from_axis_angle(state.fwd, self.angle);
                    state.right = r * state.right;
                    state.up = r * state.up;
                }
                '/' | 'r' => {
                    let r = Quat::from_axis_angle(state.fwd, -self.angle);
                    state.right = r * state.right;
                    state.up = r * state.up;
                }
                // ── Reverse ───────────────────────────────────────────────
                '|' => {
                    let r = Quat::from_axis_angle(state.up, std::f32::consts::PI);
                    state.fwd = r * state.fwd;
                    state.right = r * state.right;
                }
                // ── Push (enter branch) ───────────────────────────────────
                '[' => {
                    stack.push(state.clone());
                    state.depth += 1;
                    state.radius *= self.radius_scale;
                    state.step *= self.length_scale;
                }
                // ── Pop (exit branch, place leaf at tip) ──────────────────
                ']' => {
                    // Leaf at the tip of the sub-branch (before restoring position).
                    let leaf_pos = state.pos;
                    let leaf_size = (state.radius * 6.0 + 0.25).clamp(0.2, 1.8);
                    let leaf_dep = state.depth;

                    if let Some(saved) = stack.pop() {
                        state = saved;
                    }
                    // Only render leaves at depth ≥ 2 (suppress trunk forks).
                    if leaf_dep >= 2 {
                        result.leaves.push(LeafCluster {
                            position: leaf_pos,
                            size: leaf_size,
                            depth: leaf_dep,
                        });
                    }
                }
                // ── Explicit leaf placement ────────────────────────────────
                'L' => {
                    result.leaves.push(LeafCluster {
                        position: state.pos,
                        size: (state.radius * 6.0 + 0.25).clamp(0.2, 1.8),
                        depth: state.depth,
                    });
                }
                // ── Thin branch ───────────────────────────────────────────
                '!' => {
                    state.radius = (state.radius * self.radius_scale).max(0.02);
                }
                // ── All other symbols (grammar variables X Y Z etc.) ──────
                _ => {}
            }
        }

        result
    }
}

// ── Geometry helper ───────────────────────────────────────────────────────────

/// Compute the quaternion that rotates Vec3::Y onto `dir`.
/// Used to orient a Bevy `Cylinder` (Y-up by default) along a branch direction.
pub fn orient_along(dir: Vec3) -> Quat {
    let dir = dir.normalize_or_zero();
    if dir.length_squared() < 0.001 {
        return Quat::IDENTITY;
    }
    let up = Vec3::Y;
    let dot = dir.dot(up);
    if (dot - 1.0).abs() < 1e-4 {
        Quat::IDENTITY
    } else if (dot + 1.0).abs() < 1e-4 {
        Quat::from_rotation_z(std::f32::consts::PI)
    } else {
        Quat::from_rotation_arc(up, dir)
    }
}
