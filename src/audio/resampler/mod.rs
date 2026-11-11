//! Band-limited step synthesis: the mixer's level is a staircase on the M-cycle clock, and each
//! step goes into the output-rate buffer as a windowed-sinc impulse in a delta buffer, which is
//! then integrated. The integrator leaks, which makes it a one-pole DC blocker for free. A one-pole
//! low pass follows, for the treble a DMG's amplifier and speaker take off.

use std::sync::OnceLock;
use crate::audio::sample::AudioSample;
use crate::cycles::MachineCycles;

#[cfg(test)]
mod tests;

/// Taps per impulse, so a step is spread over this many output samples.
const TAPS: usize = 16;
/// Sub-sample positions the kernel is tabulated at; a step between two is interpolated.
const PHASE_BITS: u32 = 5;
const PHASES: usize = 1 << PHASE_BITS;
/// Output samples from a level change to the middle of its step.
const LATENCY: usize = TAPS / 2 - 1;
/// The kernel's -6 dB point, in cycles per output sample. With this width the Kaiser window's
/// transition band reaches its stopband by Nyquist, so nothing above it folds back.
const CUTOFF: f64 = 0.35;
const KAISER_BETA: f64 = 7.0;
const DEFAULT_BASS_HZ: f64 = 28.0;
/// The output stage's treble corner. `pokered`'s synth rolls off at the same frequency so that
/// swapping one backend for the other does not change the sound.
const TREBLE_ROLLOFF_HZ: f64 = 8_250.0;
const DEFAULT_SAMPLE_RATE: u32 = 44_100;
/// One output sample on the 32.32 clock.
const ONE: u64 = 1 << 32;

/// The impulse at each sub-sample phase, and each row's difference to the next. Every row sums
/// to one, so an interpolated step settles at precisely the level it was asked for.
struct Kernel {
    rows: [[f32; TAPS]; PHASES],
    slopes: [[f32; TAPS]; PHASES],
}

impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Kernel({TAPS} taps x {PHASES} phases)")
    }
}

impl Kernel {
    fn shared() -> &'static Kernel {
        static KERNEL: OnceLock<Kernel> = OnceLock::new();
        KERNEL.get_or_init(Kernel::build)
    }

    fn build() -> Kernel {
        let half = TAPS as f64 / 2.0;
        let mut table = [[0.0f64; TAPS]; PHASES + 1];
        for (phase, row) in table.iter_mut().enumerate() {
            let frac = phase as f64 / PHASES as f64;
            // Tap `k` lands on output sample `floor(t) + k`, so it sits `k - frac` after the step.
            let taps: Vec<f64> = (0..TAPS)
                .map(|k| {
                    let x = k as f64 - frac - LATENCY as f64;
                    sinc(2.0 * CUTOFF * x) * kaiser(x / half, KAISER_BETA)
                })
                .collect();
            let sum: f64 = taps.iter().sum();
            for (out, tap) in row.iter_mut().zip(&taps) {
                *out = tap / sum;
            }
        }
        let mut kernel = Kernel { rows: [[0.0; TAPS]; PHASES], slopes: [[0.0; TAPS]; PHASES] };
        for phase in 0..PHASES {
            for k in 0..TAPS {
                kernel.rows[phase][k] = table[phase][k] as f32;
                kernel.slopes[phase][k] = (table[phase + 1][k] - table[phase][k]) as f32;
            }
        }
        kernel
    }
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) }
}

/// Kaiser window over `u` in [-1, 1], lowered to meet zero at its ends: a kernel that jumped there
/// would click each time a step's position crossed an output sample.
fn kaiser(u: f64, beta: f64) -> f64 {
    if u.abs() >= 1.0 {
        return 0.0;
    }
    (bessel_i0(beta * (1.0 - u * u).sqrt()) - 1.0) / (bessel_i0(beta) - 1.0)
}

/// Modified Bessel function of the first kind, order zero, by its power series.
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > sum * 1e-12 {
        term *= (x / (2.0 * k)) * (x / (2.0 * k));
        sum += term;
        k += 1.0;
    }
    sum
}

/// Turns the mixer's stereo level, sampled on the M-cycle clock, into PCM at the sink's rate.
///
/// Time is 32.32 fixed point in output samples, so the output depends only on when each level
/// changed and never on how the clock was split between [`Self::end_frame`] calls.
#[derive(Debug, Clone)]
pub struct StereoResampler {
    clock_rate: u32,
    sample_rate: u32,
    speed: f64,
    bass_hz: f64,
    /// Output-sample time per M-cycle, 32.32.
    step: u64,
    /// Now, 32.32, counted from `buf[0]`. Kept below `capacity` samples by [`Self::end_frame`].
    time: u64,
    /// `[0, ready)` are finished output samples; from `ready` on, step deltas still to integrate.
    buf: Vec<[f32; 2]>,
    ready: usize,
    /// Most output samples held unread; the buffer is this plus one kernel's width.
    capacity: usize,
    acc: [f32; 2],
    /// The integrator's leak per output sample.
    decay: f32,
    /// The treble low pass's state and its step towards the input per output sample.
    treble: [f32; 2],
    treble_gain: f32,
    level: AudioSample,
    kernel: &'static Kernel,
    #[cfg(test)]
    capture: Option<Vec<(u16, i16, i16)>>,
}

impl Default for StereoResampler {
    fn default() -> Self {
        Self::new((MachineCycles::CPU_FREQ / 4) as u32, DEFAULT_SAMPLE_RATE)
    }
}

impl StereoResampler {
    pub fn new(clock_rate: u32, sample_rate: u32) -> Self {
        let mut resampler = Self {
            clock_rate: clock_rate.max(1),
            sample_rate: 0,
            speed: 1.0,
            bass_hz: DEFAULT_BASS_HZ,
            step: 0,
            time: 0,
            buf: Vec::new(),
            ready: 0,
            capacity: 0,
            acc: [0.0; 2],
            decay: 1.0,
            treble: [0.0; 2],
            treble_gain: 1.0,
            level: AudioSample::ZERO,
            kernel: Kernel::shared(),
            #[cfg(test)]
            capture: None,
        };
        resampler.set_sample_rate(sample_rate);
        resampler
    }

    /// Retune to a new output rate, which empties the buffer.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate.max(1);
        // A tenth of a second, which every sink drains well inside.
        self.capacity = (self.sample_rate as usize / 10).max(TAPS);
        self.buf = vec![[0.0; 2]; self.capacity + TAPS];
        self.retune();
        self.clear();
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// How fast the clock runs against real time: at 2.0 an emulated second makes half a second
    /// of output.
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
        self.retune();
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// The DC blocker's corner; zero integrates without a leak.
    #[cfg(test)]
    pub fn set_bass_freq(&mut self, hz: f64) {
        self.bass_hz = hz;
        self.retune();
    }

    fn retune(&mut self) {
        let ratio = self.sample_rate as f64 / (self.clock_rate as f64 * self.speed);
        // At most one output sample per M-cycle, so `clocks * step` cannot overflow.
        self.step = if ratio.is_finite() && ratio > 0.0 {
            (ratio * ONE as f64).round().min(ONE as f64) as u64
        } else {
            ONE
        };
        self.decay = (-2.0 * std::f64::consts::PI * self.bass_hz / self.sample_rate as f64).exp() as f32;
        self.treble_gain = 1.0 - (-2.0 * std::f64::consts::PI * TREBLE_ROLLOFF_HZ / self.sample_rate as f64).exp() as f32;
    }

    /// Drop everything buffered and start again from silence.
    pub fn clear(&mut self) {
        self.buf.fill([0.0; 2]);
        self.ready = 0;
        self.time = 0;
        self.acc = [0.0; 2];
        self.treble = [0.0; 2];
        self.level = AudioSample::ZERO;
    }

    /// The level from now on.
    #[inline]
    pub fn update(&mut self, level: AudioSample) {
        if level == self.level {
            return;
        }
        let (dl, dr) = (level.left - self.level.left, level.right - self.level.right);
        self.level = level;
        #[cfg(test)]
        self.capture_level();

        let start = (self.time >> 32) as usize;
        let frac = self.time as u32;
        let phase = (frac >> (32 - PHASE_BITS)) as usize;
        let within = (frac << PHASE_BITS) as f32 * (1.0 / ONE as f32);
        let (row, slope) = (&self.kernel.rows[phase], &self.kernel.slopes[phase]);
        for ((out, &tap), &rise) in self.buf[start..start + TAPS].iter_mut().zip(row).zip(slope) {
            let tap = tap + within * rise;
            out[0] += dl * tap;
            out[1] += dr * tap;
        }
    }

    /// Move the clock on by `clocks` M-cycles.
    #[inline]
    pub fn end_frame(&mut self, clocks: u32) {
        self.time = self.time.saturating_add(clocks as u64 * self.step);
        #[cfg(test)]
        self.capture_clocks(clocks);
        if self.time >= (self.capacity as u64) << 32 {
            self.overflow();
        }
    }

    /// Integrate every sample the clock has passed. No later level change can reach them.
    fn integrate(&mut self) {
        let end = ((self.time >> 32) as usize).min(self.buf.len());
        let [mut l, mut r] = self.acc;
        let [mut tl, mut tr] = self.treble;
        for out in &mut self.buf[self.ready..end] {
            l = l * self.decay + out[0];
            r = r * self.decay + out[1];
            tl += self.treble_gain * (l - tl);
            tr += self.treble_gain * (r - tr);
            *out = [tl, tr];
        }
        self.acc = [l, r];
        self.treble = [tl, tr];
        self.ready = self.ready.max(end);
    }

    /// Nobody has read for a whole buffer: keep the newest half and drop the rest.
    #[cold]
    fn overflow(&mut self) {
        let end = (self.time >> 32) as usize;
        let keep = self.capacity / 2;
        self.integrate();
        if end < self.buf.len() + keep {
            self.discard(end - keep);
            // The samples past the buffer's end had no steps pending, so they are the leak alone.
            let [mut l, mut r] = self.acc;
            let [mut tl, mut tr] = self.treble;
            for out in &mut self.buf[self.ready..keep] {
                l *= self.decay;
                r *= self.decay;
                tl += self.treble_gain * (l - tl);
                tr += self.treble_gain * (r - tr);
                *out = [tl, tr];
            }
            self.acc = [l, r];
            self.treble = [tl, tr];
            self.ready = self.ready.max(keep);
            return;
        }
        // One call moved far past the buffer, so nothing in it is new enough to keep.
        let skipped = (end - self.buf.len()).min(i32::MAX as usize) as i32;
        for acc in &mut self.acc {
            *acc *= self.decay.powi(skipped);
        }
        self.treble = self.acc;
        self.buf.fill([0.0; 2]);
        self.ready = 0;
        self.time &= ONE - 1;
    }

    /// Remove the oldest `n` finished samples.
    fn discard(&mut self, n: usize) {
        let live = ((self.time >> 32) as usize + TAPS).min(self.buf.len());
        self.buf.copy_within(n..live, 0);
        self.buf[live - n..live].fill([0.0; 2]);
        self.ready -= n;
        self.time -= (n as u64) << 32;
    }

    /// Fill `out` with interleaved L/R frames, returning how many frames were written.
    pub fn read_interleaved_f32(&mut self, out: &mut [f32]) -> usize {
        self.integrate();
        let n = self.ready.min(out.len() / 2);
        for (pair, sample) in out.chunks_exact_mut(2).zip(&self.buf[..n]) {
            pair.copy_from_slice(sample);
        }
        if n > 0 {
            self.discard(n);
        }
        n
    }

    /// Log every level handed in from now on, run-length merged: one `(M-cycles held, left,
    /// right)` per level, with a run longer than a `u16` split across entries.
    #[cfg(test)]
    pub fn start_capture(&mut self) {
        let (l, r) = (quantise(self.level.left), quantise(self.level.right));
        self.capture = Some(vec![(0, l, r)]);
    }

    #[cfg(test)]
    pub fn take_capture(&mut self) -> Vec<(u16, i16, i16)> {
        self.capture.take().unwrap_or_default()
    }

    #[cfg(test)]
    fn capture_level(&mut self) {
        let (l, r) = (quantise(self.level.left), quantise(self.level.right));
        if let Some(log) = &mut self.capture
            && log.last().is_none_or(|&(_, pl, pr)| (pl, pr) != (l, r))
        {
            log.push((0, l, r));
        }
    }

    #[cfg(test)]
    fn capture_clocks(&mut self, mut clocks: u32) {
        let Some(log) = &mut self.capture else { return };
        while clocks > 0 {
            let last = log.last_mut().expect("a capture starts with an entry");
            let room = (u16::MAX - last.0) as u32;
            if room == 0 {
                let (_, l, r) = *last;
                log.push((0, l, r));
                continue;
            }
            let add = clocks.min(room);
            last.0 += add as u16;
            clocks -= add;
        }
    }
}

#[cfg(test)]
fn quantise(level: f32) -> i16 {
    (level * 32767.0).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}
