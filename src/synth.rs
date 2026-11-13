//! Every sound and all five music loops, generated from code at startup, so
//! the game ships as one file with nothing to license. Music loops wrap
//! notes around the loop end, so they repeat without a seam.

use std::f32::consts::TAU;

use crate::game::Rng;

pub const SAMPLE_RATE: u32 = 22_050;
const SR: f32 = SAMPLE_RATE as f32;

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Tri,
    Square,
    Saw,
}

fn wave(w: Wave, phase: f32) -> f32 {
    match w {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Tri => 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0,
        Wave::Square => {
            if phase.fract() < 0.5 {
                0.6
            } else {
                -0.6
            }
        }
        Wave::Saw => 2.0 * phase.fract() - 1.0,
    }
}

fn midi(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// A note: frequency sweeps from `f0` to `f1`, with an attack then exponential decay.
#[derive(Clone, Copy)]
struct Note {
    f0: f32,
    f1: f32,
    amp: f32,
    wave: Wave,
    attack: f32,
    decay: f32,
    dur: f32,
    vibrato: f32,
}

impl Note {
    fn new(freq: f32, dur: f32, amp: f32, wave: Wave) -> Note {
        Note {
            f0: freq,
            f1: freq,
            amp,
            wave,
            attack: 0.005,
            decay: dur / 3.0,
            dur,
            vibrato: 0.0,
        }
    }

    fn sweep(mut self, to: f32) -> Note {
        self.f1 = to;
        self
    }

    fn env(mut self, attack: f32, decay: f32) -> Note {
        self.attack = attack;
        self.decay = decay;
        self
    }

    fn vibrato(mut self, depth: f32) -> Note {
        self.vibrato = depth;
        self
    }
}

fn add(buf: &mut [f32], start: f32, n: Note) {
    let len = buf.len();
    let s0 = (start * SR) as usize;
    let total = (n.dur * SR) as usize;
    let mut phase = 0.0f32;
    for i in 0..total {
        let t = i as f32 / SR;
        let k = t / n.dur;
        let env = if t < n.attack {
            t / n.attack
        } else {
            (-(t - n.attack) / n.decay).exp()
        };
        let tail = ((total - i) as f32 / (0.01 * SR)).min(1.0);
        let f = (n.f0 + (n.f1 - n.f0) * k) * (1.0 + n.vibrato * (t * 5.5 * TAU).sin());
        phase = (phase + f / SR).fract();
        buf[(s0 + i) % len] += wave(n.wave, phase) * env * tail * n.amp;
    }
}

/// Continuous tone over the whole loop; frequency is rounded so it lands on a
/// whole number of cycles and the loop point is seamless.
fn drone(buf: &mut [f32], freq: f32, amp: f32, wave_kind: Wave, swell_cycles: f32) {
    let secs = buf.len() as f32 / SR;
    let f = (freq * secs).round() / secs;
    for (i, s) in buf.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let swell = 0.6 + 0.4 * (t / secs * swell_cycles * TAU).sin();
        *s += wave(wave_kind, (f * t).fract()) * amp * swell;
    }
}

fn noise_burst(
    buf: &mut [f32],
    rng: &mut Rng,
    start: f32,
    dur: f32,
    amp: f32,
    decay: f32,
    smooth: f32,
) {
    let len = buf.len();
    let s0 = (start * SR) as usize;
    let mut lp = 0.0f32;
    for i in 0..(dur * SR) as usize {
        let t = i as f32 / SR;
        lp += (rng.range(-1.0, 1.0) - lp) * smooth;
        buf[(s0 + i) % len] += lp * amp * (-t / decay).exp();
    }
}

fn lowpass(buf: &mut [f32], smooth: f32) {
    // Run twice around the loop so the filter state is settled at the seam.
    let mut lp = 0.0f32;
    for _ in 0..2 {
        for s in buf.iter_mut() {
            lp += (*s - lp) * smooth;
            *s = lp;
        }
    }
}

fn echo(buf: &mut [f32], delay: f32, feedback: f32) {
    let len = buf.len();
    let d = (delay * SR) as usize;
    for _ in 0..3 {
        for i in 0..len {
            buf[i] += buf[(i + len - d % len) % len] * feedback;
        }
    }
}

fn normalize(buf: &mut [f32], peak: f32) {
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if max > 0.0 {
        buf.iter_mut().for_each(|s| *s *= peak / max);
    }
}

fn silence(secs: f32) -> Vec<f32> {
    vec![0.0; (secs * SR) as usize]
}

/// Wraps samples in a 16-bit mono WAV file.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// The loop for a depth band, from bright (0) to ominous (4).
pub fn music(band: usize) -> Vec<f32> {
    let mut rng = Rng::new(0xC0FFEE + band as u64);
    let mut buf;
    match band {
        0 => {
            // Surface: bright major I-V-vi-IV with a plucky arpeggio.
            let beat = 60.0 / 112.0;
            buf = silence(32.0 * beat);
            let chords: [[f32; 3]; 4] = [
                [60.0, 64.0, 67.0],
                [55.0, 59.0, 62.0],
                [57.0, 60.0, 64.0],
                [53.0, 57.0, 60.0],
            ];
            let pent = [72.0, 74.0, 76.0, 79.0, 81.0];
            for bar in 0..8 {
                let chord = chords[bar % 4];
                let t0 = bar as f32 * 4.0 * beat;
                for b in 0..4 {
                    let root = if b % 2 == 0 { chord[0] } else { chord[2] } - 24.0;
                    add(
                        &mut buf,
                        t0 + b as f32 * beat,
                        Note::new(midi(root), beat, 0.22, Wave::Tri).env(0.01, 0.25),
                    );
                }
                for e in 0..8 {
                    let note = chord[e % 3] + if e >= 4 { 12.0 } else { 0.0 };
                    add(
                        &mut buf,
                        t0 + e as f32 * beat / 2.0,
                        Note::new(midi(note), 0.3, 0.1, Wave::Tri).env(0.003, 0.09),
                    );
                    if e % 2 == 1 {
                        noise_burst(
                            &mut buf,
                            &mut rng,
                            t0 + e as f32 * beat / 2.0,
                            0.05,
                            0.03,
                            0.015,
                            0.9,
                        );
                    }
                }
                for q in 0..4 {
                    if rng.f32() < 0.55 {
                        let n = pent[(rng.next_u64() % 5) as usize];
                        add(
                            &mut buf,
                            t0 + q as f32 * beat,
                            Note::new(midi(n), beat * 0.9, 0.09, Wave::Sine)
                                .env(0.01, 0.3)
                                .vibrato(0.004),
                        );
                    }
                }
            }
        }
        1 => {
            // Shallow: curious minor, gentle plucks with echo and a soft pad.
            let beat = 60.0 / 96.0;
            buf = silence(32.0 * beat);
            let chords: [[f32; 3]; 4] = [
                [57.0, 60.0, 64.0],
                [53.0, 57.0, 60.0],
                [48.0, 52.0, 55.0],
                [55.0, 59.0, 62.0],
            ];
            for bar in 0..8 {
                let chord = chords[bar % 4];
                let t0 = bar as f32 * 4.0 * beat;
                add(
                    &mut buf,
                    t0,
                    Note::new(midi(chord[0] - 24.0), 4.0 * beat, 0.2, Wave::Sine).env(0.05, 1.5),
                );
                for c in chord {
                    add(
                        &mut buf,
                        t0,
                        Note::new(midi(c), 4.0 * beat, 0.035, Wave::Tri).env(0.8, 2.5),
                    );
                }
                for q in 0..4 {
                    if rng.f32() < 0.7 {
                        let note = chord[(rng.next_u64() % 3) as usize] + 12.0;
                        add(
                            &mut buf,
                            t0 + q as f32 * beat,
                            Note::new(midi(note), 0.5, 0.1, Wave::Sine).env(0.004, 0.15),
                        );
                    }
                }
            }
            echo(&mut buf, beat * 0.75, 0.3);
        }
        2 => {
            // Mid: slow, sparse, dripping; plinks echo in the dark.
            let beat = 60.0 / 76.0;
            buf = silence(32.0 * beat);
            let chords: [[f32; 3]; 4] = [
                [50.0, 53.0, 57.0],
                [46.0, 50.0, 53.0],
                [43.0, 46.0, 50.0],
                [45.0, 49.0, 52.0],
            ];
            for bar in 0..8 {
                let chord = chords[bar % 4];
                let t0 = bar as f32 * 4.0 * beat;
                add(
                    &mut buf,
                    t0,
                    Note::new(midi(chord[0] - 12.0), 4.0 * beat, 0.16, Wave::Sine).env(0.6, 2.5),
                );
                for c in chord {
                    add(
                        &mut buf,
                        t0,
                        Note::new(midi(c), 4.0 * beat, 0.03, Wave::Saw).env(1.2, 2.0),
                    );
                }
                for e in 0..8 {
                    if rng.f32() < 0.3 {
                        let note = chord[(rng.next_u64() % 3) as usize] + 24.0;
                        add(
                            &mut buf,
                            t0 + e as f32 * beat / 2.0,
                            Note::new(midi(note), 0.4, 0.08, Wave::Sine).env(0.002, 0.1),
                        );
                    }
                }
                if rng.f32() < 0.6 {
                    let f = rng.range(1400.0, 2200.0);
                    add(
                        &mut buf,
                        t0 + rng.range(0.0, 4.0 * beat),
                        Note::new(f, 0.08, 0.06, Wave::Sine).sweep(f * 0.5),
                    );
                }
            }
            lowpass(&mut buf, 0.5);
            echo(&mut buf, beat * 1.5, 0.4);
        }
        3 => {
            // Deep: tritone drone, slow heartbeat, creaking rock.
            let beat = 1.0;
            buf = silence(24.0 * beat);
            drone(&mut buf, midi(36.0), 0.18, Wave::Sine, 2.0);
            drone(&mut buf, midi(42.0), 0.06, Wave::Tri, 3.0);
            for b in 0..24 {
                let t = b as f32 * beat;
                if b % 2 == 0 {
                    add(
                        &mut buf,
                        t,
                        Note::new(70.0, 0.25, 0.4, Wave::Sine)
                            .sweep(40.0)
                            .env(0.005, 0.08),
                    );
                    add(
                        &mut buf,
                        t + 0.28,
                        Note::new(60.0, 0.25, 0.3, Wave::Sine)
                            .sweep(35.0)
                            .env(0.005, 0.08),
                    );
                }
                if rng.f32() < 0.25 {
                    let at = t + rng.range(0.0, 1.0);
                    noise_burst(&mut buf, &mut rng, at, 0.8, 0.12, 0.3, 0.02);
                }
                if b % 6 == 3 {
                    add(
                        &mut buf,
                        t,
                        Note::new(midi(49.0), 3.0, 0.05, Wave::Saw).env(1.0, 1.5),
                    );
                    add(
                        &mut buf,
                        t,
                        Note::new(midi(50.0), 3.0, 0.05, Wave::Saw).env(1.0, 1.5),
                    );
                }
            }
            lowpass(&mut buf, 0.3);
        }
        _ => {
            // Abyss: sub drone, wind, a fast heartbeat and a thin whine.
            let beat = 60.0 / 52.0;
            buf = silence(24.0 * beat);
            drone(&mut buf, midi(30.0), 0.22, Wave::Sine, 1.0);
            drone(&mut buf, midi(30.3), 0.12, Wave::Sine, 2.0);
            drone(&mut buf, midi(42.0), 0.05, Wave::Saw, 3.0);
            for b in 0..24 {
                let t = b as f32 * beat;
                add(
                    &mut buf,
                    t,
                    Note::new(65.0, 0.22, 0.45, Wave::Sine)
                        .sweep(38.0)
                        .env(0.004, 0.07),
                );
                add(
                    &mut buf,
                    t + 0.24,
                    Note::new(55.0, 0.22, 0.35, Wave::Sine)
                        .sweep(32.0)
                        .env(0.004, 0.07),
                );
                noise_burst(&mut buf, &mut rng, t, beat, 0.1, beat, 0.01);
            }
            for _ in 0..3 {
                let t = rng.range(0.0, 20.0 * beat);
                add(
                    &mut buf,
                    t,
                    Note::new(1760.0, 4.0, 0.025, Wave::Sine)
                        .env(2.0, 1.5)
                        .vibrato(0.01),
                );
                add(
                    &mut buf,
                    t,
                    Note::new(1865.0, 4.0, 0.02, Wave::Sine).env(2.0, 1.5),
                );
            }
            lowpass(&mut buf, 0.35);
        }
    }
    normalize(&mut buf, 0.8);
    buf
}

/// Named sound effects. Loops (`drill`, `thrust`) repeat seamlessly.
pub fn sfx(name: &str) -> Vec<f32> {
    let mut rng = Rng::new(
        name.bytes()
            .fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64)),
    );
    let mut buf;
    match name {
        "drill" => {
            buf = silence(0.5);
            drone(&mut buf, 76.0, 0.3, Wave::Saw, 8.0);
            drone(&mut buf, 152.0, 0.12, Wave::Square, 16.0);
            noise_burst(&mut buf, &mut rng, 0.0, 0.5, 0.25, 100.0, 0.4);
            lowpass(&mut buf, 0.5);
        }
        "thrust" => {
            buf = silence(0.5);
            noise_burst(&mut buf, &mut rng, 0.0, 0.5, 0.8, 100.0, 0.08);
            drone(&mut buf, 56.0, 0.15, Wave::Sine, 4.0);
        }
        "dug" => {
            buf = silence(0.25);
            noise_burst(&mut buf, &mut rng, 0.0, 0.2, 0.7, 0.05, 0.3);
            add(
                &mut buf,
                0.0,
                Note::new(110.0, 0.15, 0.4, Wave::Sine).sweep(60.0),
            );
        }
        "sell" => {
            buf = silence(0.9);
            add(
                &mut buf,
                0.0,
                Note::new(1320.0, 0.08, 0.3, Wave::Square).env(0.002, 0.04),
            );
            add(
                &mut buf,
                0.08,
                Note::new(1760.0, 0.08, 0.3, Wave::Square).env(0.002, 0.04),
            );
            add(
                &mut buf,
                0.14,
                Note::new(2093.0, 0.7, 0.35, Wave::Sine).env(0.002, 0.25),
            );
            add(
                &mut buf,
                0.14,
                Note::new(3136.0, 0.7, 0.15, Wave::Sine).env(0.002, 0.2),
            );
        }
        "buy" => {
            buf = silence(0.7);
            for (i, n) in [72.0, 76.0, 79.0, 84.0].iter().enumerate() {
                add(
                    &mut buf,
                    i as f32 * 0.07,
                    Note::new(midi(*n), 0.35, 0.3, Wave::Tri).env(0.003, 0.12),
                );
            }
        }
        "deny" => {
            buf = silence(0.35);
            add(
                &mut buf,
                0.0,
                Note::new(220.0, 0.12, 0.25, Wave::Square).env(0.002, 0.08),
            );
            add(
                &mut buf,
                0.15,
                Note::new(165.0, 0.18, 0.25, Wave::Square).env(0.002, 0.1),
            );
        }
        "low_fuel" => {
            buf = silence(0.4);
            add(
                &mut buf,
                0.0,
                Note::new(880.0, 0.1, 0.25, Wave::Square).env(0.002, 0.2),
            );
            add(
                &mut buf,
                0.18,
                Note::new(880.0, 0.1, 0.25, Wave::Square).env(0.002, 0.2),
            );
        }
        "hit" => {
            buf = silence(0.4);
            add(
                &mut buf,
                0.0,
                Note::new(170.0, 0.3, 0.6, Wave::Sine).sweep(45.0),
            );
            noise_burst(&mut buf, &mut rng, 0.0, 0.25, 0.5, 0.06, 0.4);
        }
        "land" => {
            buf = silence(0.2);
            add(
                &mut buf,
                0.0,
                Note::new(120.0, 0.15, 0.4, Wave::Sine).sweep(55.0),
            );
            noise_burst(&mut buf, &mut rng, 0.0, 0.1, 0.2, 0.03, 0.3);
        }
        "death" => {
            buf = silence(1.4);
            noise_burst(&mut buf, &mut rng, 0.0, 1.3, 0.8, 0.35, 0.15);
            add(
                &mut buf,
                0.0,
                Note::new(400.0, 1.0, 0.35, Wave::Tri)
                    .sweep(70.0)
                    .env(0.005, 0.5),
            );
        }
        "respawn" => {
            buf = silence(0.9);
            add(
                &mut buf,
                0.0,
                Note::new(220.0, 0.6, 0.3, Wave::Sine)
                    .sweep(880.0)
                    .env(0.05, 0.4),
            );
            for i in 0..5 {
                add(
                    &mut buf,
                    0.3 + i as f32 * 0.06,
                    Note::new(midi(84.0 + i as f32 * 3.0), 0.2, 0.12, Wave::Sine),
                );
            }
        }
        "rumble" => {
            buf = silence(3.0);
            let mut rumble = silence(3.0);
            noise_burst(&mut rumble, &mut rng, 0.0, 3.0, 1.0, 100.0, 0.01);
            for (i, s) in rumble.iter().enumerate() {
                let t = i as f32 / SR;
                let swell = (t / 1.2).min(1.0) * ((3.0 - t) / 0.8).min(1.0);
                buf[i] = s * swell * (1.0 + 0.5 * (t * 7.0 * TAU).sin());
            }
            add(
                &mut buf,
                0.0,
                Note::new(38.0, 3.0, 0.5, Wave::Sine)
                    .env(1.2, 1.5)
                    .vibrato(0.05),
            );
        }
        "stash" => {
            buf = silence(0.9);
            for (i, n) in [72.0, 74.0, 76.0, 79.0, 81.0, 84.0, 86.0, 88.0]
                .iter()
                .enumerate()
            {
                add(
                    &mut buf,
                    i as f32 * 0.045,
                    Note::new(midi(*n), 0.3, 0.18, Wave::Sine).env(0.002, 0.1),
                );
            }
        }
        "full" => {
            buf = silence(0.4);
            add(&mut buf, 0.0, Note::new(660.0, 0.12, 0.2, Wave::Tri));
            add(&mut buf, 0.13, Note::new(440.0, 0.2, 0.2, Wave::Tri));
        }
        _ if name.starts_with("pickup_") => {
            let tier = name[7..].parse::<f32>().unwrap_or(0.0);
            buf = silence(0.8);
            let base = [72.0, 74.0, 76.0, 79.0, 81.0, 84.0][(tier as usize).min(5)];
            add(
                &mut buf,
                0.0,
                Note::new(midi(base), 0.5, 0.3, Wave::Sine).env(0.002, 0.18),
            );
            add(
                &mut buf,
                0.0,
                Note::new(midi(base + 12.0), 0.4, 0.1, Wave::Sine).env(0.002, 0.12),
            );
            add(
                &mut buf,
                0.07,
                Note::new(midi(base + 7.0), 0.5, 0.22, Wave::Sine).env(0.002, 0.18),
            );
            for i in 0..tier as usize {
                add(
                    &mut buf,
                    0.14 + i as f32 * 0.04,
                    Note::new(midi(base + 12.0 + 2.0 * i as f32), 0.25, 0.08, Wave::Sine),
                );
            }
        }
        _ if name.starts_with("growl_") => {
            let size = name[6..].parse::<usize>().unwrap_or(0);
            let base = [260.0, 160.0, 95.0, 55.0][size.min(3)];
            let dur = 0.8 + 0.3 * size as f32;
            buf = silence(dur + 0.1);
            add(
                &mut buf,
                0.0,
                Note::new(base, dur, 0.5, Wave::Saw)
                    .sweep(base * 0.7)
                    .env(0.15, dur)
                    .vibrato(0.06),
            );
            add(
                &mut buf,
                0.0,
                Note::new(base * 1.5, dur, 0.2, Wave::Saw)
                    .sweep(base)
                    .env(0.2, dur * 0.6)
                    .vibrato(0.08),
            );
            noise_burst(&mut buf, &mut rng, 0.0, dur, 0.2, dur, 0.1);
            lowpass(&mut buf, 0.25);
        }
        _ => buf = silence(0.05),
    }
    normalize(&mut buf, 0.9);
    buf
}

/// Every effect name `sfx` knows, for loading them all up front.
pub fn sfx_names() -> Vec<String> {
    let fixed = [
        "drill", "thrust", "dug", "sell", "buy", "deny", "low_fuel", "hit", "land", "death",
        "respawn", "rumble", "stash", "full",
    ];
    let mut names: Vec<String> = fixed.iter().map(|s| s.to_string()).collect();
    names.extend((0..6).map(|t| format!("pickup_{t}")));
    names.extend((0..4).map(|s| format!("growl_{s}")));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The WAV header has to describe the data exactly or the decoder rejects it.
    #[test]
    fn wav_header_matches_data() {
        let bytes = wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]),
            6
        );
        assert_eq!(bytes.len(), 44 + 6);
    }

    /// Every sound and loop is audible and never clips.
    #[test]
    fn every_sound_is_audible_and_in_range() {
        let mut all: Vec<(String, Vec<f32>)> = sfx_names()
            .into_iter()
            .map(|n| (n.clone(), sfx(&n)))
            .collect();
        all.extend((0..5).map(|b| (format!("music_{b}"), music(b))));
        for (name, buf) in all {
            let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.1 && peak <= 1.0, "{name} peak {peak}");
            assert!(buf.iter().all(|s| s.is_finite()), "{name} has NaN");
        }
    }
}
