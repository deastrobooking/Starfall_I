//! Procedural retro SFX synthesizer (Sprint 3 — "break the silence").
//!
//! Zero external assets: every combat/UI sound is rendered at startup from an
//! [`SfxParams`] recipe into 16-bit mono WAV bytes that Bevy's `AudioSource`
//! plays directly. Pure functions, deterministic output (noise uses a seeded
//! LCG, never `thread_rng`), so renders are unit-testable byte-for-byte and a
//! future mod pipeline can expose the recipes as data.
//!
//! The palette is deliberately Secret-of-Mana-era: square/saw sweeps, noise
//! bursts, and a soft bit-crush for the 16-bit console grain.

/// Output sample rate. 22.05 kHz keeps the retro character and the buffers tiny.
pub const SAMPLE_RATE: u32 = 22_050;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Square,
    Saw,
    Triangle,
    /// Seeded white noise (percussion/impacts).
    Noise,
}

/// A complete one-shot recipe: pitch sweep + envelope + colour.
#[derive(Clone, Copy, Debug)]
pub struct SfxParams {
    pub waveform: Waveform,
    /// Start/end frequency in Hz; swept exponentially across the duration.
    pub freq_start: f32,
    pub freq_end: f32,
    pub duration: f32,
    /// Linear attack time (seconds) to full gain.
    pub attack: f32,
    /// Exponential decay rate after the attack (higher = snappier).
    pub decay: f32,
    /// Output gain 0..1.
    pub gain: f32,
    /// Quantization steps for bit-crush grain (0 = off; 16-64 = crunchy).
    pub crush_steps: u16,
    /// Blend of seeded noise on top of the tonal waveform (0..1).
    pub noise_mix: f32,
}

impl SfxParams {
    const fn tone(waveform: Waveform, f0: f32, f1: f32, duration: f32) -> Self {
        Self {
            waveform,
            freq_start: f0,
            freq_end: f1,
            duration,
            attack: 0.004,
            decay: 6.0,
            gain: 0.8,
            crush_steps: 48,
            noise_mix: 0.0,
        }
    }
}

/// Render a recipe to a complete RIFF/WAVE file (16-bit PCM mono).
pub fn render_wav(params: &SfxParams) -> Vec<u8> {
    let samples = render_samples(params);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = (samples.len() * 2) as u32;

    // RIFF header.
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    // fmt chunk: PCM, mono, 16-bit.
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
                                                 // data chunk.
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn render_samples(params: &SfxParams) -> Vec<i16> {
    let count = ((params.duration * SAMPLE_RATE as f32) as usize).max(8);
    let mut samples = Vec::with_capacity(count);
    let mut phase = 0.0_f32;
    // Deterministic noise: LCG seeded from the recipe so renders are stable.
    let mut noise_state: u32 = 0x9E37_79B9 ^ (params.freq_start as u32).wrapping_mul(2_654_435_761);

    let f0 = params.freq_start.max(1.0);
    let f1 = params.freq_end.max(1.0);

    for i in 0..count {
        let t = i as f32 / count as f32;
        let time = i as f32 / SAMPLE_RATE as f32;

        // Exponential pitch sweep reads far more "chip" than linear.
        let freq = f0 * (f1 / f0).powf(t);
        phase = (phase + freq / SAMPLE_RATE as f32).fract();

        let tone = match params.waveform {
            Waveform::Sine => (phase * std::f32::consts::TAU).sin(),
            Waveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Saw => phase * 2.0 - 1.0,
            Waveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
            Waveform::Noise => next_noise(&mut noise_state),
        };
        let mixed = if params.noise_mix > 0.0 && params.waveform != Waveform::Noise {
            tone * (1.0 - params.noise_mix) + next_noise(&mut noise_state) * params.noise_mix
        } else {
            tone
        };

        // Envelope: linear attack, exponential decay.
        let attack = (time / params.attack.max(1e-4)).min(1.0);
        let decay = (-params.decay * time).exp();
        // Short release ramp at the tail kills end-clicks.
        let release = ((1.0 - t) * 24.0).min(1.0);
        let mut value = mixed * attack * decay * release * params.gain;

        if params.crush_steps > 1 {
            let steps = params.crush_steps as f32;
            value = (value * steps).round() / steps;
        }

        samples.push((value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
    }
    samples
}

fn next_noise(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*state >> 16) & 0xFFFF) as f32 / 32_768.0 - 1.0
}

// ── The retro palette ─────────────────────────────────────────────────────────

pub fn preset_shoot() -> SfxParams {
    SfxParams {
        gain: 0.5,
        decay: 22.0,
        ..SfxParams::tone(Waveform::Square, 960.0, 220.0, 0.09)
    }
}

pub fn preset_slash() -> SfxParams {
    SfxParams {
        noise_mix: 0.55,
        gain: 0.62,
        decay: 16.0,
        ..SfxParams::tone(Waveform::Saw, 1400.0, 320.0, 0.11)
    }
}

pub fn preset_hit() -> SfxParams {
    SfxParams {
        noise_mix: 0.35,
        gain: 0.7,
        decay: 18.0,
        ..SfxParams::tone(Waveform::Square, 210.0, 70.0, 0.08)
    }
}

pub fn preset_parry() -> SfxParams {
    SfxParams {
        gain: 0.75,
        decay: 7.0,
        crush_steps: 0,
        ..SfxParams::tone(Waveform::Sine, 1180.0, 1660.0, 0.16)
    }
}

pub fn preset_kill() -> SfxParams {
    SfxParams {
        noise_mix: 0.30,
        gain: 0.8,
        decay: 6.5,
        ..SfxParams::tone(Waveform::Square, 520.0, 52.0, 0.30)
    }
}

pub fn preset_hurt() -> SfxParams {
    SfxParams {
        gain: 0.72,
        decay: 10.0,
        ..SfxParams::tone(Waveform::Square, 170.0, 58.0, 0.17)
    }
}

pub fn preset_loot() -> SfxParams {
    SfxParams {
        gain: 0.55,
        decay: 9.0,
        crush_steps: 0,
        ..SfxParams::tone(Waveform::Triangle, 660.0, 1320.0, 0.13)
    }
}

pub fn preset_chest() -> SfxParams {
    SfxParams {
        gain: 0.6,
        decay: 5.0,
        crush_steps: 0,
        ..SfxParams::tone(Waveform::Triangle, 392.0, 1568.0, 0.28)
    }
}

pub fn preset_level_up() -> SfxParams {
    SfxParams {
        gain: 0.7,
        decay: 3.2,
        attack: 0.01,
        crush_steps: 0,
        ..SfxParams::tone(Waveform::Saw, 440.0, 1760.0, 0.45)
    }
}

pub fn preset_reload() -> SfxParams {
    SfxParams {
        noise_mix: 0.5,
        gain: 0.4,
        decay: 14.0,
        ..SfxParams::tone(Waveform::Triangle, 300.0, 620.0, 0.10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid_riff_pcm() {
        let bytes = render_wav(&preset_shoot());
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        // Declared sizes must match actual byte counts.
        let riff_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff_len + 8, bytes.len());
        let data_len = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        assert_eq!(data_len + 44, bytes.len());
    }

    #[test]
    fn renders_are_audible_and_deterministic() {
        for params in [
            preset_shoot(),
            preset_slash(),
            preset_hit(),
            preset_parry(),
            preset_kill(),
            preset_hurt(),
            preset_loot(),
            preset_chest(),
            preset_level_up(),
            preset_reload(),
        ] {
            let a = render_wav(&params);
            let b = render_wav(&params);
            assert_eq!(a, b, "renders must be deterministic");
            // Non-silence: at least one sample above ~6% full scale.
            let loud = a[44..]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs())
                .any(|s| s > 2000);
            assert!(loud, "preset rendered silence");
        }
    }

    #[test]
    fn envelope_tail_avoids_click() {
        let bytes = render_wav(&preset_kill());
        let last = bytes[bytes.len() - 2..].try_into().unwrap();
        let final_sample = i16::from_le_bytes(last);
        assert!(
            final_sample.unsigned_abs() < 1500,
            "tail must ramp to ~zero"
        );
    }
}
