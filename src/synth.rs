//! Every sound and all five music loops, generated from code at startup, so
//! the game ships as one file with nothing to license. Anything that rings
//! past a loop's end (notes, echoes, reverb) wraps around to its start, so the
//! loops repeat without a seam.

use std::f32::consts::TAU;

use crate::game::Rng;

// NOTE: matches the mixer's own rate. quad-snd resamples anything else with a
// nearest-neighbor pick, which sounds gritty.
pub const SAMPLE_RATE: u32 = 44_100;
const SR: f32 = SAMPLE_RATE as f32;

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Tri,
    Square,
    Saw,
    /// A few soft harmonics: a rounded, string-or-voice-like tone.
    Warm,
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
        Wave::Warm => {
            let p = phase * TAU;
            0.6 * (p.sin() + 0.45 * (2.0 * p).sin() + 0.2 * (3.0 * p).sin() + 0.1 * (4.0 * p).sin())
        }
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
    let attack = ((n.attack * SR) as usize).max(1);
    let fall = (-1.0 / (n.decay * SR)).exp();
    let release = 0.02 * SR;
    let mut phase = 0.0f32;
    let mut env = 1.0f32;
    for i in 0..total {
        let t = i as f32 / SR;
        let level = if i < attack {
            i as f32 / attack as f32
        } else {
            env *= fall;
            if env < 1e-4 {
                break;
            }
            env
        };
        let tail = ((total - i) as f32 / release).min(1.0);
        // Vibrato eases in, the way a singer or violinist leans into a held note.
        let vib = n.vibrato * (t / 0.35).min(1.0) * (t * 5.2 * TAU).sin();
        let f = (n.f0 + (n.f1 - n.f0) * (t / n.dur)) * (1.0 + vib);
        phase = (phase + f / SR).fract();
        buf[(s0 + i) % len] += wave(n.wave, phase) * level * tail * n.amp;
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
    // One silent lap first, so the filter enters the loop already carrying the
    // loop's own ending instead of fading in from zero (that fade was a click).
    let mut lp = 0.0f32;
    for &s in buf.iter() {
        lp += (s - lp) * smooth;
    }
    for s in buf.iter_mut() {
        lp += (*s - lp) * smooth;
        *s = lp;
    }
}

fn echo(buf: &mut [f32], delay: f32, feedback: f32) {
    let len = buf.len();
    let d = (delay * SR) as usize % len;
    let dry = buf.to_vec();
    // Each pass adds one more repeat, reading the previous pass in full, so
    // echoes from the loop's end land at its start exactly as they would mid-loop.
    for _ in 0..6 {
        let prev = buf.to_vec();
        for i in 0..len {
            buf[i] = dry[i] + feedback * prev[(i + len - d) % len];
        }
    }
}

/// Schroeder reverb (four damped combs into two allpasses) run for two laps of
/// the loop, so the tail of the last bars rings on into the first ones.
fn reverb(buf: &mut [f32], mix: f32, size: f32) {
    const COMBS: [f32; 4] = [0.0297, 0.0371, 0.0411, 0.0437];
    const ALLPASS: [f32; 2] = [0.0050, 0.0017];
    let len = buf.len();
    let feedback = 0.80 + 0.08 * size;
    let mut wet = vec![0.0f32; len];
    for c in COMBS {
        let d = ((c * (1.0 + size)) * SR) as usize;
        let mut line = vec![0.0f32; d];
        let (mut idx, mut lp) = (0, 0.0f32);
        for lap in 0..2 {
            for i in 0..len {
                let out = line[idx];
                lp = out * 0.75 + lp * 0.25;
                line[idx] = buf[i] + lp * feedback;
                idx = (idx + 1) % d;
                if lap == 1 {
                    wet[i] += out;
                }
            }
        }
    }
    for a in ALLPASS {
        let d = (a * SR) as usize;
        let src = wet.clone();
        let mut line = vec![0.0f32; d];
        let mut idx = 0;
        for lap in 0..2 {
            for i in 0..len {
                let delayed = line[idx];
                let out = -src[i] + delayed;
                line[idx] = src[i] + delayed * 0.5;
                idx = (idx + 1) % d;
                if lap == 1 {
                    wet[i] = out;
                }
            }
        }
    }
    for (s, w) in buf.iter_mut().zip(&wet) {
        *s = *s * (1.0 - 0.4 * mix) + w * mix * 0.3;
    }
}

/// Folds the last `secs` of a buffer over its start, so a loop with
/// non-repeating content (noise) still flows from its end into its start.
fn crossfade_loop(buf: Vec<f32>, secs: f32) -> Vec<f32> {
    let overlap = (secs * SR) as usize;
    let keep = buf.len() - overlap;
    let mut out = buf[..keep].to_vec();
    for (i, s) in out.iter_mut().take(overlap).enumerate() {
        let k = i as f32 / overlap as f32;
        *s = *s * k + buf[keep + i] * (1.0 - k);
    }
    out
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

/// A held chord: two slightly detuned warm voices per note, slow swell, and a
/// tail that rings well past the bar so chords bleed into each other.
fn pad(buf: &mut [f32], start: f32, dur: f32, notes: &[f32], amp: f32) {
    for &n in notes {
        for detune in [0.997, 1.003] {
            add(
                buf,
                start,
                Note::new(midi(n) * detune, dur * 1.8, amp, Wave::Warm)
                    .env(dur * 0.35, dur * 0.9)
                    .vibrato(0.002),
            );
        }
    }
}

/// A sung melody note: warm tone plus a soft octave, delayed vibrato, long tail.
fn sing(buf: &mut [f32], start: f32, note: f32, beats: f32, beat: f32, amp: f32) {
    let dur = beats * beat;
    add(
        buf,
        start,
        Note::new(midi(note), dur * 2.5, amp, Wave::Warm)
            .env(0.05, dur * 0.9)
            .vibrato(0.006),
    );
    add(
        buf,
        start,
        Note::new(midi(note + 12.0), dur * 2.0, amp * 0.25, Wave::Sine)
            .env(0.08, dur * 0.7)
            .vibrato(0.006),
    );
}

/// A plucked string (harp or guitar-ish) with a ringing tail.
fn pluck(buf: &mut [f32], start: f32, note: f32, amp: f32, ring: f32) {
    add(
        buf,
        start,
        Note::new(midi(note), ring * 4.0, amp, Wave::Tri).env(0.003, ring),
    );
    add(
        buf,
        start,
        Note::new(midi(note + 12.0), ring * 2.0, amp * 0.3, Wave::Sine).env(0.002, ring * 0.5),
    );
}

/// A bell: a sine plus an inharmonic partial, the thing that makes it ring "metal".
fn bell(buf: &mut [f32], start: f32, note: f32, amp: f32) {
    add(
        buf,
        start,
        Note::new(midi(note), 4.0, amp, Wave::Sine).env(0.002, 1.4),
    );
    add(
        buf,
        start,
        Note::new(midi(note) * 2.76, 2.0, amp * 0.35, Wave::Sine).env(0.002, 0.5),
    );
}

/// A big soft drum hit for phrase starts: the "epic" in the journey.
fn timpani(buf: &mut [f32], rng: &mut Rng, start: f32, amp: f32) {
    add(
        buf,
        start,
        Note::new(98.0, 1.6, amp, Wave::Sine)
            .sweep(68.0)
            .env(0.004, 0.45),
    );
    add(
        buf,
        start,
        Note::new(147.0, 0.8, amp * 0.3, Wave::Sine)
            .sweep(110.0)
            .env(0.004, 0.2),
    );
    noise_burst(buf, rng, start, 0.25, amp * 0.25, 0.05, 0.15);
}

fn heartbeat(buf: &mut [f32], start: f32, amp: f32) {
    add(
        buf,
        start,
        Note::new(70.0, 0.3, amp, Wave::Sine)
            .sweep(40.0)
            .env(0.005, 0.09),
    );
    add(
        buf,
        start + 0.28,
        Note::new(60.0, 0.3, amp * 0.75, Wave::Sine)
            .sweep(35.0)
            .env(0.005, 0.09),
    );
}

/// Picks melody notes near the chord from a scale: mostly steps, sometimes leaps.
fn tune(rng: &mut Rng, scale: &[f32], chord_root: f32, last: &mut usize) -> f32 {
    let step: i32 = match rng.next_u64() % 6 {
        0 => -2,
        1 | 2 => -1,
        3 | 4 => 1,
        _ => 2,
    };
    let mut i = (*last as i32 + step).clamp(0, scale.len() as i32 - 1) as usize;
    // Drift back toward the chord so the line stays anchored.
    if rng.f32() < 0.25 {
        i = scale
            .iter()
            .position(|&n| (n - chord_root) % 12.0 == 0.0)
            .unwrap_or(i);
    }
    *last = i;
    scale[i]
}

fn surface(rng: &mut Rng) -> Vec<f32> {
    // Setting out: warm major I-V-vi-IV, harp arpeggios, a singing line, drums at phrase starts.
    let beat = 60.0 / 92.0;
    let mut buf = silence(64.0 * beat);
    let chords: [[f32; 3]; 4] = [
        [60.0, 64.0, 67.0],
        [55.0, 59.0, 62.0],
        [57.0, 60.0, 64.0],
        [53.0, 57.0, 60.0],
    ];
    let scale = [67.0, 69.0, 72.0, 74.0, 76.0, 79.0, 81.0, 84.0];
    let mut last = 2;
    for bar in 0..16 {
        let chord = chords[bar % 4];
        let t0 = bar as f32 * 4.0 * beat;
        add(
            &mut buf,
            t0,
            Note::new(midi(chord[0] - 24.0), 4.0 * beat, 0.2, Wave::Warm).env(0.02, 1.2),
        );
        add(
            &mut buf,
            t0 + 2.0 * beat,
            Note::new(midi(chord[2] - 24.0), 2.0 * beat, 0.14, Wave::Warm).env(0.02, 0.8),
        );
        pad(&mut buf, t0, 4.0 * beat, &chord, 0.035);
        for e in 0..8 {
            let note = chord[[0, 1, 2, 1][e % 4]] + if e >= 4 { 12.0 } else { 0.0 };
            pluck(&mut buf, t0 + e as f32 * beat / 2.0, note, 0.05, 0.35);
            if e % 2 == 1 {
                noise_burst(
                    &mut buf,
                    rng,
                    t0 + e as f32 * beat / 2.0,
                    0.05,
                    0.02,
                    0.015,
                    0.9,
                );
            }
        }
        if bar % 8 == 0 {
            timpani(&mut buf, rng, t0, 0.35);
        }
        // Melody rests in the first two bars of each half, then sings.
        if bar % 8 >= 2 {
            let lift = if bar >= 8 { 12.0 } else { 0.0 };
            let mut b = 0.0;
            while b < 4.0 {
                let len = if rng.f32() < 0.6 { 1.0 } else { 2.0 };
                if rng.f32() < 0.8 {
                    let n = tune(rng, &scale, chord[0], &mut last) + lift - 12.0;
                    sing(&mut buf, t0 + b * beat, n, len, beat, 0.09);
                }
                b += len;
            }
        }
    }
    reverb(&mut buf, 0.35, 0.6);
    buf
}

fn shallow(rng: &mut Rng) -> Vec<f32> {
    // Wonder: minor key, rolling harp, a flute-like line floating over it.
    let beat = 60.0 / 80.0;
    let mut buf = silence(64.0 * beat);
    let chords: [[f32; 3]; 4] = [
        [57.0, 60.0, 64.0],
        [53.0, 57.0, 60.0],
        [48.0, 52.0, 55.0],
        [55.0, 59.0, 62.0],
    ];
    let scale = [69.0, 72.0, 74.0, 76.0, 79.0, 81.0, 84.0];
    let mut last = 3;
    for bar in 0..16 {
        let chord = chords[bar % 4];
        let t0 = bar as f32 * 4.0 * beat;
        add(
            &mut buf,
            t0,
            Note::new(midi(chord[0] - 24.0), 4.0 * beat, 0.2, Wave::Sine).env(0.05, 2.0),
        );
        pad(&mut buf, t0, 4.0 * beat, &chord, 0.04);
        for e in 0..8 {
            let up = [0, 1, 2, 3, 2, 1, 0, 1][e];
            let note = if up == 3 { chord[0] + 12.0 } else { chord[up] } + 12.0;
            pluck(&mut buf, t0 + e as f32 * beat / 2.0, note, 0.045, 0.7);
        }
        if bar % 8 == 0 {
            timpani(&mut buf, rng, t0, 0.25);
        }
        if bar % 2 == 1 || bar >= 8 {
            let mut b = 0.0;
            while b < 4.0 {
                let len = if rng.f32() < 0.5 { 2.0 } else { 1.0 };
                if rng.f32() < 0.75 {
                    let n = tune(rng, &scale, chord[0], &mut last);
                    sing(&mut buf, t0 + b * beat, n, len, beat, 0.075);
                }
                b += len;
            }
        }
    }
    echo(&mut buf, beat * 0.75, 0.25);
    reverb(&mut buf, 0.45, 0.75);
    buf
}

fn middle(rng: &mut Rng) -> Vec<f32> {
    // Into the dark: low strings, a slow cello line, bells and drips in a big cave.
    let beat = 60.0 / 68.0;
    let mut buf = silence(64.0 * beat);
    let chords: [[f32; 3]; 4] = [
        [50.0, 53.0, 57.0],
        [46.0, 50.0, 53.0],
        [43.0, 46.0, 50.0],
        [45.0, 49.0, 52.0],
    ];
    let scale = [45.0, 48.0, 50.0, 52.0, 53.0, 55.0, 57.0, 60.0];
    let mut last = 2;
    for bar in 0..16 {
        let chord = chords[bar % 4];
        let t0 = bar as f32 * 4.0 * beat;
        add(
            &mut buf,
            t0,
            Note::new(midi(chord[0] - 12.0), 4.0 * beat, 0.18, Wave::Warm)
                .env(0.6, 2.5)
                .vibrato(0.002),
        );
        pad(&mut buf, t0, 4.0 * beat, &chord, 0.045);
        if bar % 4 == 0 {
            timpani(&mut buf, rng, t0, 0.22);
        }
        if bar >= 4 {
            let mut b = 0.0;
            while b < 4.0 {
                let len = if rng.f32() < 0.6 { 2.0 } else { 4.0 };
                let n = tune(rng, &scale, chord[0], &mut last);
                sing(&mut buf, t0 + b * beat, n, len, beat, 0.1);
                b += len;
            }
        }
        for e in 0..8 {
            if rng.f32() < 0.18 {
                bell(
                    &mut buf,
                    t0 + e as f32 * beat / 2.0,
                    chord[(rng.next_u64() % 3) as usize] + 24.0,
                    0.05,
                );
            }
        }
        if rng.f32() < 0.6 {
            let f = rng.range(1400.0, 2200.0);
            add(
                &mut buf,
                t0 + rng.range(0.0, 4.0 * beat),
                Note::new(f, 0.08, 0.04, Wave::Sine).sweep(f * 0.5),
            );
        }
    }
    echo(&mut buf, beat * 1.5, 0.3);
    reverb(&mut buf, 0.5, 0.9);
    buf
}

fn deep(rng: &mut Rng) -> Vec<f32> {
    // Deep: tritone drone, slow heartbeat, low brass swelling up out of the rock.
    let beat = 1.0;
    let mut buf = silence(32.0 * beat);
    drone(&mut buf, midi(36.0), 0.16, Wave::Sine, 2.0);
    drone(&mut buf, midi(42.0), 0.05, Wave::Warm, 3.0);
    for b in 0..32 {
        let t = b as f32 * beat;
        if b % 2 == 0 {
            heartbeat(&mut buf, t, 0.4);
        }
        if rng.f32() < 0.2 {
            let at = t + rng.range(0.0, 1.0);
            noise_burst(&mut buf, rng, at, 0.8, 0.1, 0.3, 0.02);
        }
        if b % 8 == 4 {
            for n in [36.0, 43.0, 48.0] {
                add(
                    &mut buf,
                    t,
                    Note::new(midi(n), 6.0, 0.07, Wave::Warm)
                        .env(2.0, 2.5)
                        .vibrato(0.003),
                );
            }
        }
        if b % 8 == 0 {
            add(
                &mut buf,
                t,
                Note::new(midi(49.0), 5.0, 0.04, Wave::Warm)
                    .env(1.5, 2.0)
                    .vibrato(0.004),
            );
            add(
                &mut buf,
                t,
                Note::new(midi(50.0), 5.0, 0.04, Wave::Warm)
                    .env(1.5, 2.0)
                    .vibrato(0.004),
            );
        }
    }
    lowpass(&mut buf, 0.35);
    reverb(&mut buf, 0.55, 1.0);
    buf
}

fn abyss(rng: &mut Rng) -> Vec<f32> {
    // Abyss: sub drone, wind, a fast heartbeat, dissonant swells and a thin whine.
    let beat = 60.0 / 52.0;
    let mut buf = silence(32.0 * beat);
    drone(&mut buf, midi(30.0), 0.22, Wave::Sine, 1.0);
    drone(&mut buf, midi(30.3), 0.12, Wave::Sine, 2.0);
    drone(&mut buf, midi(42.0), 0.04, Wave::Warm, 3.0);
    for b in 0..32 {
        let t = b as f32 * beat;
        heartbeat(&mut buf, t, 0.45);
        noise_burst(&mut buf, rng, t, beat, 0.08, beat, 0.01);
        if b % 8 == 2 {
            for n in [42.0, 43.0, 49.0] {
                add(
                    &mut buf,
                    t,
                    Note::new(midi(n), 7.0, 0.05, Wave::Warm)
                        .env(3.0, 2.5)
                        .vibrato(0.008),
                );
            }
        }
    }
    for _ in 0..4 {
        let t = rng.range(0.0, 28.0 * beat);
        add(
            &mut buf,
            t,
            Note::new(1760.0, 5.0, 0.02, Wave::Sine)
                .env(2.0, 2.0)
                .vibrato(0.01),
        );
        add(
            &mut buf,
            t,
            Note::new(1865.0, 5.0, 0.016, Wave::Sine).env(2.0, 2.0),
        );
    }
    lowpass(&mut buf, 0.4);
    reverb(&mut buf, 0.65, 1.0);
    buf
}

/// The loop for a depth band, from bright (0) to ominous (4).
pub fn music(band: usize) -> Vec<f32> {
    let mut rng = Rng::new(0xC0FFEE + band as u64);
    let mut buf = match band {
        0 => surface(&mut rng),
        1 => shallow(&mut rng),
        2 => middle(&mut rng),
        3 => deep(&mut rng),
        _ => abyss(&mut rng),
    };
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
            buf = silence(0.55);
            drone(&mut buf, 76.0, 0.3, Wave::Saw, 8.0);
            drone(&mut buf, 152.0, 0.12, Wave::Square, 16.0);
            noise_burst(&mut buf, &mut rng, 0.0, 0.55, 0.25, 100.0, 0.4);
            lowpass(&mut buf, 0.5);
            buf = crossfade_loop(buf, 0.05);
        }
        "thrust" => {
            buf = silence(0.55);
            noise_burst(&mut buf, &mut rng, 0.0, 0.55, 0.8, 100.0, 0.08);
            drone(&mut buf, 56.0, 0.15, Wave::Sine, 4.0);
            buf = crossfade_loop(buf, 0.05);
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

    /// The jump from a loop's last sample back to its first must be no bigger
    /// than the ordinary sample-to-sample steps inside it, or it clicks on every repeat.
    #[test]
    fn loops_have_no_click_at_the_seam() {
        let mut loops: Vec<(String, Vec<f32>)> =
            (0..5).map(|b| (format!("music_{b}"), music(b))).collect();
        loops.extend(["drill", "thrust"].map(|n| (n.to_string(), sfx(n))));
        for (name, buf) in loops {
            let mut steps: Vec<f32> = buf.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            steps.sort_by(f32::total_cmp);
            let typical = steps[steps.len() * 99 / 100];
            let seam = (buf[0] - buf[buf.len() - 1]).abs();
            assert!(
                seam <= typical,
                "{name} seam jump {seam} vs 99th percentile step {typical}"
            );
        }
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
