# Fixed-Tick Motor (EC1) — how simulation and input flow

**Default ON.** The player simulation runs at `FIXED_HZ` (64) in `FixedUpdate`;
rendering stays uncapped. Escape hatches: `STARFALL_LEGACY_MOTOR=1` or F10.

## The pipeline

```
render frame:  device input → PlayerInput (PreUpdate, input_plugin)
               └→ latch_player_input          (RunFixedMainLoop, BEFORE fixed loop)
fixed tick:    consume_fixed_input → PlayerInputBuffers::fixed(idx)   [input_buffer.rs]
               cache_previous_tick_positions  (interpolation source)
               traversal → grapple → player_movement → loop/checkpoint → impact
               (each writes translation into PlayerMovement.motor_accum)
render frame:  flush_motor_translation        (PostUpdate, before physics controller)
               player_camera_follow_system    (lerps by Time<Fixed>::overstep_fraction())
```

Key invariants:
- **Edges fire exactly once per tick.** A tap that lands between ticks is
  latched (OR-accumulated) and consumed on the next tick — never dropped,
  never double-fired. See `FixedInput::overlay()` and its unit tests.
- **Physics steps per frame**, so per-tick translations are *summed* in
  `PlayerMovement.motor_accum` and flushed once per frame. Never write
  `controller.translation` directly from a fixed system.
- **Camera interpolation** hides the 64 Hz staircase above 64 fps by lerping
  from `PreviousTickPosition` toward the live transform (≤1 tick of camera
  latency, motor-path only).

## Adding a simulation system

1. Register it in BOTH chains in `player_plugin.rs` (`build()`): the
   `FixedUpdate` chain (`run_if(fixed_motor_on)`) and the legacy `Update`
   chain (`run_if(fixed_motor_off)`), in the same relative order.
2. Gate it with `.run_if(hitstop_inactive)` unless it must run during hit
   pauses (feedback/presentation systems do; sim systems don't).
3. Read input through the seam, not the component:
   ```rust
   let pi = if sim.fixed_motor {
       buffered = buffers.fixed(idx.0).map(|f| f.overlay(pi)).unwrap_or_else(|| pi.clone());
       &buffered
   } else { pi };
   ```
   For a single edge, `buffers.fixed(idx.0).map(|f| f.edges.X)` is lighter.
4. Use `time.delta_secs()` for integration — inside `FixedUpdate` Bevy hands
   you the fixed clock automatically; the same code is frame-based on the
   legacy path.
5. New buffered actions: add the field to `HeldInput` (held/analog) or
   `EdgeInput` (one-shot) in `input_buffer.rs`, accumulate it in
   `EdgeInput::accumulate`, project it in `FixedInput::overlay`, and extend
   the overlay unit test.

## Testing

`cargo test --bin starfall-i input_buffer` covers latch/consume/overlay.
Feel-test with F10 A/B at your display's refresh rate; watch `sim: N ticks
@64Hz` on the F11 overlay.
