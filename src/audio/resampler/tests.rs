use super::*;

const CLOCK: u32 = 1 << 20;
const RATE: u32 = 48_000;

fn drain(resampler: &mut StereoResampler) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    let mut scratch = [0.0f32; 1024];
    loop {
        let n = resampler.read_interleaved_f32(&mut scratch);
        if n == 0 {
            return out;
        }
        out.extend(scratch[..n * 2].chunks_exact(2).map(|p| [p[0], p[1]]));
    }
}

fn left(frames: &[[f32; 2]]) -> Vec<f32> {
    frames.iter().map(|f| f[0]).collect()
}

/// Run `clocks` M-cycles in slices small enough that nothing overflows, reading as it goes.
fn run(resampler: &mut StereoResampler, clocks: u64, out: &mut Vec<[f32; 2]>) {
    let mut left = clocks;
    while left > 0 {
        let slice = left.min(1000);
        resampler.end_frame(slice as u32);
        left -= slice;
        out.extend(drain(resampler));
    }
}

/// A stretch of silence, read out, so the next step starts the output.
fn settle_silence(resampler: &mut StereoResampler) {
    let per_sample = CLOCK as f64 / RATE as f64;
    let clocks = (per_sample * 100.0).round() as u64;
    let mut sink = Vec::new();
    run(resampler, clocks, &mut sink);
}

#[test]
fn every_kernel_phase_sums_to_one() {
    for row in &Kernel::shared().rows {
        let sum: f64 = row.iter().map(|&t| t as f64).sum();
        assert!((sum - 1.0).abs() < 1e-6, "a phase sums to {sum}");
    }
}

#[test]
fn a_step_settles_at_its_level_and_is_centred_on_the_latency() {
    let mut resampler = StereoResampler::new(CLOCK, RATE);
    resampler.set_bass_freq(0.0);
    settle_silence(&mut resampler);
    let before = drain(&mut resampler).len();
    assert_eq!(before, 0);

    // The step lands at an exact output-sample boundary, so its midpoint is LATENCY + 1/2 later.
    let at = resampler.time;
    resampler.update(AudioSample::new(0.5, -0.25));
    let mut out = Vec::new();
    run(&mut resampler, CLOCK as u64 / 4, &mut out);
    let (l, r) = (left(&out), out.iter().map(|f| f[1]).collect::<Vec<_>>());
    let frac = (at & (ONE - 1)) as f64 / ONE as f64;
    let midpoint = LATENCY as f64 + frac;
    let crossing = l.iter().position(|&v| v >= 0.25).unwrap() as f64;
    assert!((crossing - midpoint).abs() <= 1.0, "midpoint at {crossing}, expected {midpoint}");

    // An exact level once the kernel has passed and the treble pole, which shrinks what is left
    // to a third each sample, has caught up.
    for (i, (&a, &b)) in l.iter().zip(&r).enumerate().skip(2 * TAPS) {
        assert!((a - 0.5).abs() < 1e-5 && (b + 0.25).abs() < 1e-5, "sample {i} is ({a}, {b})");
    }
    let peak = l.iter().cloned().fold(0.0f32, f32::max);
    assert!(peak < 0.5 * 1.1, "the step rings to {peak}");
}

#[test]
fn the_dc_blocker_returns_a_held_level_to_zero() {
    let mut resampler = StereoResampler::new(CLOCK, RATE);
    settle_silence(&mut resampler);
    resampler.update(AudioSample::new(0.5, 0.5));
    let mut out = Vec::new();
    run(&mut resampler, CLOCK as u64, &mut out);
    let l = left(&out);

    let peak = l.iter().cloned().fold(0.0f32, f32::max);
    assert!(peak > 0.45, "the step barely arrived: {peak}");
    // One time constant after the step it has fallen to 1/e.
    let tau = RATE as f64 / (2.0 * std::f64::consts::PI * DEFAULT_BASS_HZ);
    let at_tau = l[LATENCY + tau.round() as usize] as f64;
    assert!((at_tau / 0.5 - (-1.0f64).exp()).abs() < 0.02, "one time constant in: {at_tau}");
    assert!(l[l.len() - 1].abs() < 1e-4, "a second later it is still at {}", l[l.len() - 1]);
}

/// Power at `freq` Hz, Hann windowed, by Goertzel.
fn power_at(samples: &[f32], freq: f64) -> f64 {
    let n = samples.len();
    let w = 2.0 * std::f64::consts::PI * freq / RATE as f64;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for (i, &x) in samples.iter().enumerate() {
        let hann = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
        let s = x as f64 * hann + coeff * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}

/// A square wave of `half_period` M-cycles per half, read back as the left channel.
fn square(half_period: u32, frames: usize) -> Vec<f32> {
    let mut resampler = StereoResampler::new(CLOCK, RATE);
    let mut out = Vec::new();
    let mut high = true;
    while out.len() < frames + 4800 {
        let level = if high { 0.25 } else { -0.25 };
        resampler.update(AudioSample::new(level, level));
        resampler.end_frame(half_period);
        high = !high;
        out.extend(drain(&mut resampler));
    }
    // Past the DC blocker's start-up.
    left(&out[4800..4800 + frames])
}

/// Where a harmonic lands once sampled at `RATE`.
fn folded(freq: f64) -> f64 {
    let rate = RATE as f64;
    let f = freq % rate;
    if f > rate / 2.0 { rate - f } else { f }
}

#[test]
fn harmonics_above_nyquist_do_not_fold_back() {
    // About 3.5 kHz, so the 7th harmonic and up are past Nyquist and would fold into the band.
    let half_period = 150;
    let fundamental = CLOCK as f64 / (2 * half_period) as f64;
    let wave = square(half_period, 16384);
    let reference = power_at(&wave, fundamental);
    assert!(reference > 0.0);

    // In band, and through: the 3rd harmonic is where a square wave puts it.
    let third = power_at(&wave, 3.0 * fundamental) / reference;
    assert!((third.log10() * 10.0 + 9.5).abs() < 6.0, "3rd harmonic at {:.1} dB", third.log10() * 10.0);

    let nyquist = RATE as f64 / 2.0;
    let in_band: Vec<f64> = (1..).step_by(2).map(|h| h as f64 * fundamental).take_while(|&f| f < nyquist).collect();
    for harmonic in (1..120).step_by(2).filter(|&h| h as f64 * fundamental > nyquist) {
        let alias = folded(harmonic as f64 * fundamental);
        // A fold that lands on an in-band harmonic is not measurable as aliasing.
        if in_band.iter().any(|&f| (alias - f).abs() < 50.0) {
            continue;
        }
        let db = 10.0 * (power_at(&wave, alias) / reference).log10();
        assert!(db < -70.0, "harmonic {harmonic} folds to {alias:.0} Hz at {db:.1} dB");
    }
}

#[test]
fn a_tone_above_nyquist_is_all_but_silent() {
    // 32 kHz at a 48 kHz output: every harmonic is out of band.
    let wave = square(16, 16384);
    let rms = (wave.iter().map(|&x| (x * x) as f64).sum::<f64>() / wave.len() as f64).sqrt();
    assert!(rms < 0.25 * 10f64.powf(-65.0 / 20.0), "{rms} RMS out of a 0.25 square");
}

#[test]
fn an_audible_square_keeps_its_level() {
    let wave = square(1192, 48_000); // about 440 Hz
    let rms = (wave.iter().map(|&x| (x * x) as f64).sum::<f64>() / wave.len() as f64).sqrt();
    assert!((rms - 0.25).abs() < 0.01, "{rms} RMS out of a 0.25 square");
}

/// A cheap deterministic stream of slice lengths and levels.
fn lcg(state: &mut u64) -> u32 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (*state >> 33) as u32
}

#[test]
fn frame_count_follows_the_clock_without_drift() {
    for (rate, speed) in [(48_000, 1.0), (44_100, 1.0), (32_000, 2.0), (48_000, 0.5), (44_100, 3.7)] {
        let mut resampler = StereoResampler::new(CLOCK, rate);
        resampler.set_speed(speed);
        let (mut frames, mut clocks, mut seed) = (0usize, 0u64, 7u64);
        let mut scratch = [0.0f32; 2048];
        while clocks < CLOCK as u64 * 20 {
            let slice = lcg(&mut seed) % 3000 + 1;
            resampler.update(AudioSample::new((lcg(&mut seed) % 100) as f32 / 400.0, 0.0));
            resampler.end_frame(slice);
            clocks += slice as u64;
            while let n @ 1.. = resampler.read_interleaved_f32(&mut scratch) {
                frames += n;
            }
        }
        let expected = clocks as f64 * rate as f64 / (CLOCK as f64 * speed);
        assert!((frames as f64 - expected).abs() <= 1.0, "{rate} Hz at {speed}x: {frames} frames, expected {expected}");
    }
}

#[test]
fn how_the_clock_is_sliced_does_not_change_a_bit() {
    let render = |max_slice: u32| {
        let mut resampler = StereoResampler::new(CLOCK, RATE);
        let (mut seed, mut out) = (11u64, Vec::new());
        // The same transitions at the same instants, whatever the slicing in between.
        for _ in 0..4000 {
            let level = (lcg(&mut seed) % 200) as f32 / 400.0 - 0.25;
            resampler.update(AudioSample::new(level, -level));
            let mut gap = lcg(&mut seed) % 500 + 1;
            while gap > 0 {
                let slice = gap.min(max_slice);
                resampler.end_frame(slice);
                gap -= slice;
            }
            out.extend(drain(&mut resampler));
        }
        out
    };
    let (fine, coarse) = (render(1), render(4096));
    assert!(fine.len() > 40_000);
    assert_eq!(fine, coarse);
}

#[test]
fn set_speed_and_set_sample_rate_take_effect() {
    let frames = |resampler: &mut StereoResampler| {
        let mut out = Vec::new();
        run(resampler, CLOCK as u64, &mut out);
        out.len()
    };
    let mut resampler = StereoResampler::new(CLOCK, RATE);
    assert_eq!(frames(&mut resampler), 48_000);
    resampler.set_speed(2.0);
    assert_eq!(resampler.speed(), 2.0);
    assert_eq!(frames(&mut resampler), 24_000);
    resampler.set_speed(1.0);
    resampler.set_sample_rate(22_050);
    assert_eq!(resampler.sample_rate(), 22_050);
    assert_eq!(frames(&mut resampler), 22_050);
    assert_eq!(StereoResampler::default().sample_rate(), DEFAULT_SAMPLE_RATE);
}

#[test]
fn clear_empties_it_and_forgets_the_level() {
    let mut resampler = StereoResampler::new(CLOCK, RATE);
    resampler.update(AudioSample::new(0.5, 0.5));
    resampler.end_frame(10_000);
    resampler.clear();
    assert_eq!(resampler.read_interleaved_f32(&mut [0.0; 64]), 0);
    let mut out = Vec::new();
    run(&mut resampler, 10_000, &mut out);
    assert!(!out.is_empty() && out.iter().all(|f| *f == [0.0, 0.0]), "the old level leaked through");
}

#[test]
fn an_unread_buffer_keeps_the_newest_tenth_of_a_second() {
    let render = |read: bool| {
        let mut resampler = StereoResampler::new(CLOCK, RATE);
        let (mut seed, mut out) = (3u64, Vec::new());
        for _ in 0..2000 {
            resampler.update(AudioSample::new((lcg(&mut seed) % 100) as f32 / 400.0, 0.1));
            resampler.end_frame(lcg(&mut seed) % 1000 + 1);
            if read {
                out.extend(drain(&mut resampler));
            }
        }
        out.extend(drain(&mut resampler));
        out
    };
    let (unread, read) = (render(false), render(true));
    assert!(unread.len() <= RATE as usize / 10 && unread.len() >= RATE as usize / 20, "{} frames held", unread.len());
    assert_eq!(unread[..], read[read.len() - unread.len()..], "the frames kept are not the newest");
}

#[test]
fn a_huge_clock_jump_is_survived() {
    let mut resampler = StereoResampler::new(CLOCK, RATE);
    resampler.set_speed(f64::MIN_POSITIVE);
    resampler.update(AudioSample::new(0.5, 0.5));
    resampler.end_frame(u32::MAX);
    resampler.update(AudioSample::new(0.0, 0.0));
    resampler.end_frame(1000);
    let out = drain(&mut resampler);
    assert!(!out.is_empty() && out.len() <= RATE as usize / 10);
    assert!(out.iter().all(|f| f[0].is_finite() && f[0].abs() <= 0.6));
}

#[test]
fn capture_logs_each_level_and_how_long_it_held() {
    let mut resampler = StereoResampler::new(CLOCK, RATE);
    resampler.start_capture();
    resampler.end_frame(10);
    resampler.update(AudioSample::new(0.5, -0.5));
    resampler.end_frame(30);
    resampler.update(AudioSample::new(0.5, -0.5));
    resampler.end_frame(70_000);
    resampler.update(AudioSample::ZERO);
    resampler.end_frame(1);
    assert_eq!(
        resampler.take_capture(),
        vec![(10, 0, 0), (u16::MAX, 16384, -16384), ((70_030 - u16::MAX as u32) as u16, 16384, -16384), (1, 0, 0)],
    );
    assert!(resampler.take_capture().is_empty(), "taking the log ends the capture");
}

/// The output stage's treble pole: a square's fundamental is the same size at any pitch, so what
/// separates one at 1 kHz from one at 8 kHz is the roll-off alone.
#[test]
fn treble_rolls_off_through_one_pole() {
    let frames = 16384;
    let (low_half, high_half) = (524u32, 64u32);
    let level = |half: u32| {
        let freq = CLOCK as f64 / (2 * half) as f64;
        (freq, power_at(&square(half, frames), freq))
    };
    let (low, low_power) = level(low_half);
    let (high, high_power) = level(high_half);
    let pole = |freq: f64| {
        let gain = 1.0 - (-2.0 * std::f64::consts::PI * TREBLE_ROLLOFF_HZ / RATE as f64).exp();
        let w = 2.0 * std::f64::consts::PI * freq / RATE as f64;
        let (re, im) = (1.0 - (1.0 - gain) * w.cos(), (1.0 - gain) * w.sin());
        gain * gain / (re * re + im * im)
    };
    let measured = 10.0 * (high_power / low_power).log10();
    let expected = 10.0 * (pole(high) / pole(low)).log10();
    assert!(expected < -2.0, "the test is vacuous unless the pole is doing something: {expected:.2} dB");
    assert!((measured - expected).abs() < 1.0, "{measured:.2} dB at {high:.0} Hz against {low:.0} Hz, expected {expected:.2}");
}
