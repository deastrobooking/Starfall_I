# Starfall I — 3D Asset & Modular Level Designer Specification

> Status: forward-looking design specification. The Level Editor, glTF export,
> fitting socket, shader masking, and Star Board transformation below are not
> implemented runtime systems. See `game_review_2026-07.md` for the audited
> hoverboard, road, movement, UI, and weapon baseline.
# Feature Upgrade: Star Board Vanguard (Rocket Board Traversal)

This document serves as the implementation path and engine contract for two new adjacent tracks:
1. **The Character Armor, Weapon, and Clothing Designer** (extension of the active Character Design editor).
2. **The Modular Level Asset Designer and Route Beacon Grid**.
3. **The Star Board Vanguard system** (fast-travel traversal/backpack rocket board).

Refers to naming conventions:
- Character Customization: `CC#` milestones.
- World Level Design: `LD#` milestones.
- Rocket Board/Motion Mechanics: `MM#` milestones (extending `docs/playerengine.md`).

---

## 1. Feature Upgrade: Star Board Vanguard (Backpack-to-Rocket-Board)

The **Star Board Vanguard** is a high-speed locomotion mount that bridges the gap between active platforming and long-range exploration across the 200-mile Everest Range.

```text
    [Backpack Mode]                  [Vanguard Deploy Mode]
      [Thrusters]                      [Dual Wing Thrusters]
     [Aegis Shield]                         /        \
          |            ===[Transform]==>   o==========o (Star Board Chassis)
      [Spine Peg]                            /      \
                                         [Flame]  [Flame]
```

### 1.1 Mathematical Model
When active, the standard ground locomotion constraints in player_plugin.rs are swapped with a frictionless floating hover state:
* **Frictionless Floating Height**: The board projects a raycast downward to sample surface distance ($h$). A spring-damper suspension force offsets gravity to maintain a constant altitude ($h_{\text{target}} = 0.65\text{m}$):
  $$F_{\text{hover}} = k_{\text{spring}} \cdot (h_{\text{target}} - h) - c_{\text{damping}} \cdot v_y$$
* **High-Speed Steer Banking**: Pivoting while riding tilts the visual mesh to offer weight distribution feedback. The bank angle ($\theta_{\text{roll}}$) is dynamically scaled by yaw velocity ($\omega_{\text{yaw}}$) and current forward velocity ($v_x$):
  $$\theta_{\text{roll}} = \text{clamp}\left( -0.45, \, 0.45, \, -v_x \cdot \omega_{\text{yaw}} \cdot 1.15 \right)$$

### 1.2 Locomotion Verbs
* **Deploy/Retract**: Hold D-pad Up (or keyboard `V`) for 0.4 seconds. Plays a dynamic transformation animation: the backpack deforms, thrusters slide down, and a glowing energy grid binds under the player's feet.
* **Over-Drive Dash**: Consumes jetpack fuel rapidly to boost speed to $22.0\text{ m/s}$ along a flat plane.
* **High Jump**: Harnesses board upward thrusters to launch vertically ($14.5\text{ m/s}$), entering standard jetpack flight at the apex.

---

## 2. CC Track: Armor, Weapon, & Clothing 3D Asset Designer

The Character Designer extends the infrastructure built inside `character_design_plugin.rs`. Rather than swapping binary styles, it compiles customized attachment prefabs from data-driven recipes.

```text
  [CUSTOMIZER VIEWPORT]
  ┌────────────────────────────────────────────────────────┐
  │  [SLOT] Torso Plating    [MATERIAL] Carbon-Weave       │
  │  [SLOT] Right Pauldron   [ADDITIVE] Laser Badge [■]    │
  │  [SLOT] Left Glove       [SCALE]    1.18 [+/-]         │
  │                                                        │
  │                       ( 3D Preview )                   │
  │                      /  O (Head)  \                    │
  │                     /     | (Chest) \                  │
  │                     ------|==========                  │
  │                                                        │
  │  [Q/W/E/R] Select Slot  |  [S/D] Orbit cam  |  [Enter] │
  └────────────────────────────────────────────────────────┘
```

### 2.1 The Visual Fitting Socket Component
Every designed item inherits a target bone socket based on the humanoid skeleton declared in character.rs:

```rust
#[derive(Component, Debug, Clone)]
pub struct CustomGizmoSocket {
    pub joint: JointKind,
    pub translation_offset: Vec3,
    pub rotation_offset: Quat,
    pub attachment_scale: Vec3,
}
```

### 2.2 Functional Spawning & Material Layering
To prevent visual clipping, designed armor models use segmented base layers. When a shoulder pad is added, the system automatically runs clipping queries on nearby arms:
* **Shader Layer Masking**: standard PBR materials employ a custom `MaterialMask` flag. If `PauldronEq` is active, the shader blanks the upper arm’s skin vertices inside rendering.rs using an alpha cutoff, preventing clipping.
* **Palette Recolor Arrays**: Every compiled asset maps accent structures to three independent color vectors (`skin_override`, `fabric_primary`, `nanoty-metallic`).

---

## 3. LD Track: Modular Level Symmetrical Grid & Environment Assembler

To support clean platformer level assembly across the 200-mile Everest Range, we declare a grid-bound, block-based modular Level Design Editor (`AppState::LevelEditor`).

```text
  [ modular block placement ]
         +---------+---------+
        /    A    /    B    /|
       +---------+---------+ |
       |         |         | |
       |    C    |    D    |/
       +---------+---------+
```

### 3.1 Architectural Block Sizing Rules
To prevent collision issues, all modular blocks are built to strict dimension standards:
* **Locomotion Unit Grid**: 
  $$\text{Grid Unit } = 4.0\text{ meters}$$
* **Default Corridor Wall**: Width $4.0\text{m}$, Height $6.0\text{m}$, Depth $0.5\text{m}$.
* **Standard Stair Block**: $4.0\text{m} \times 4.0\text{m}$ bounds, with exactly 12 steps set to $0.18\text{m}$ step heights and $0.28\text{m}$ tread lengths. This matches the native `autostep_height` built into player.rs.

### 3.2 Symmetrical Geometry & Spline-Path Snapping
The Level Designer implements spatial projection alignment:
* **Symmetry Planes**: Placing a platform on the positive $X$ coordinate automatically projects a matching block on the negative $X$ plane when symmetrical mode is active, supporting quick, balanced multiplayer level design.
* **Spline Guide Snapping**: When placing high-speed route traversal rails, the designer runs a catmull-rom curve calculation to align anchors smoothly:
  $$\mathbf{P}(t) = \frac{1}{2} \left[ (2\mathbf{P}_1) + (-\mathbf{P}_0 + \mathbf{P}_2)t + (2\mathbf{P}_0 - 5\mathbf{P}_1 + 4\mathbf{P}_2 - \mathbf{P}_3)t^2 + (-\mathbf{P}_0 + 3\mathbf{P}_1 - 3\mathbf{P}_2 + \mathbf{P}_3)t^3 \right]$$
  Route beacon visual studs snap directly along this curve to provide clear player guidance in open space.

---

## 4. Implementation Steps for the Technical Team

To implement these features cleanly, follow this structural order:
1. Update **`CharacterIkPose`** to apply local roll adjustments ($\theta_{\text{roll}}$) on the pelvis joint during board flight.
2. Register **`CustomGizmoSocket`** across the custom character part spawning loops in character_parts.rs.
3. Compile Level Editor blocks into Bevy 0.19 `Mesh3d` / `MeshMaterial3d`
   render entities while saving stable recipe/content IDs in Forge project
   documents. Never serialize generated ECS entities into campaign saves.
