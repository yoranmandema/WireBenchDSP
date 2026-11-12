//! Rate conversion for a NAM model captured at a different rate than the host.
//!
//! A `.nam`'s dilations and recurrence are defined in **samples**, so feeding
//! it audio at the wrong rate produces silently wrong output. When the host
//! rate doesn't match, the block runs the model at its own rate and converts.
//!
//! Cubic (Catmull-Rom) interpolation, linear-phase, no allocation: the caller
//! provides the scratch. `upsample` and `downsample` are separate because the
//! ratio isn't an integer — the model runs for `ceil(n * ratio)` samples and
//! the result is decimated back to `n`.

/// Cubic Catmull-Rom interpolation at fractional position `t` between
/// `y1` and `y2`, using the four surrounding samples.
#[inline]
fn cubic(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let a = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
    let b = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c = -0.5 * y0 + 0.5 * y2;
    ((a * t + b) * t + c) * t + y1
}

/// Resample `input` into `output`. `ratio` is out_rate / in_rate.
///
/// `state` carries the last four input samples across blocks, ordered oldest
/// to newest: `[y_{-4}, y_{-3}, y_{-2}, y_{-1}]`. The output's sample 0 is at
/// input position 0, so the history is what a fractional position near zero
/// reads behind it.
pub fn resample(input: &[f32], output: &mut [f32], ratio: f32, state: &mut [f32; 4]) {
    if (ratio - 1.0).abs() < 1e-6 {
        let n = input.len().min(output.len());
        output[..n].copy_from_slice(&input[..n]);
        // Shift the history: the last four samples of the input.
        let start = n.saturating_sub(4);
        let tail = &input[start..n];
        for (k, s) in state.iter_mut().enumerate() {
            *s = tail.get(k.wrapping_sub(4 - tail.len())).copied().unwrap_or(0.0);
        }
        return;
    }
    let n_in = input.len();
    if n_in == 0 {
        return;
    }
    // Position in input samples for each output sample.
    let mut pos = 0.0f32;
    let step = 1.0 / ratio;
    // Four taps around the read position: y_{i-1}, y_i, y_{i+1}, y_{i+2}.
    // `hist` holds the four samples ending at the start of the block, so a
    // negative tap index maps into it.
    let hist = *state;
    let tap = |k: i64, input: &[f32]| -> f32 {
        if k >= 0 {
            input.get(k as usize).copied().unwrap_or(0.0)
        } else {
            hist[(k + 4) as usize]
        }
    };
    for out in output.iter_mut() {
        let i = pos as i64;
        let t = pos - i as f32;
        *out = cubic(tap(i - 1, input), tap(i, input), tap(i + 1, input), tap(i + 2, input), t);
        pos += step;
    }
    // The next block's history is the four samples ending where we stopped.
    let last = pos as i64 - 1;
    let mut next = [0.0f32; 4];
    for (k, slot) in next.iter_mut().enumerate() {
        *slot = tap(last - (3 - k) as i64, input);
    }
    *state = next;
}

/// Output length for an input of `n` samples at `ratio`.
#[inline]
pub fn out_len(n: usize, ratio: f32) -> usize {
    if (ratio - 1.0).abs() < 1e-6 {
        n
    } else {
        (n as f32 * ratio).ceil() as usize + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsample_midpoint() {
        // A sine at 1 kHz / 48 kHz: sample 1 is 0.1305.
        let sr = 48_000.0f32;
        let input: Vec<f32> = (0..16)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1_000.0 / sr).sin())
            .collect();
        let mut out = vec![0.0f32; 33];
        let mut state = [0.0f32; 4];
        resample(&input, &mut out, 2.0, &mut state);
        // Output 1 is half a sample in. Cubic (Catmull-Rom) interpolation of
        // 0.0, 0.1305, 0.2588 at t=0.5 gives 0.0572 — not the linear midpoint.
        let expect = cubic(0.0, input[0], input[1], input[2], 0.5);
        assert!(
            (out[1] - expect).abs() < 1e-4,
            "out[1] = {}, expected {expect}",
            out[1]
        );
        assert!(out[1] > 0.0, "interpolated midpoint should be positive");
        // Output 2 lands exactly on input sample 1.
        assert!((out[2] - input[1]).abs() < 1e-4, "out[2] = {}", out[2]);
    }

    #[test]
    fn round_trip_is_lossless_enough() {
        let sr = 48_000.0f32;
        let n = 480;
        let input: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1_000.0 / sr).sin())
            .collect();
        let mut up_state = [0.0f32; 4];
        let mut down_state = [0.0f32; 4];
        let hi_n = out_len(n, 2.0);
        let mut hi = vec![0.0f32; hi_n];
        resample(&input, &mut hi, 2.0, &mut up_state);
        let mut back = vec![0.0f32; n];
        resample(&hi, &mut back, 0.5, &mut down_state);
        let mut err = 0.0f32;
        for i in 8..n {
            err = err.max((back[i] - input[i]).abs());
        }
        assert!(err < 0.02, "round-trip error {err}");
    }
}
