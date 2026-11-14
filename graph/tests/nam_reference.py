#!/usr/bin/env python3
"""Self-contained Python reference render of the tiny NAM WaveNet.

The Neural Amp Modeler Python package is not available on this host (the PyPI
`nam` package is a different project — Google's Neural Additive Models — and
NAM's own `nam` needs torch, which is not installed). So this script implements
the same A1-style WaveNet forward pass in pure Python and compares it sample by
sample against the Rust `Nam` block's output.

The model and signal match `graph/tests/nam.rs` (`ir_model()` and
`nam_block_runs_and_passes_signal`). Run the Rust side first:

    cargo run -p dsp-graph --example nam_reference -- <outdir>
    python3 graph/tests/nam_reference.py <outdir>

Reads `<outdir>/nam_input.raw` and `<outdir>/nam_output.raw` (little-endian
f32) and exits non-zero if any sample differs by more than 1e-5.
"""

import math
import struct
import sys
from pathlib import Path

# --- the tiny model, copied from graph/tests/nam.rs::ir_model() --------------
# config: 1 array, 1 layer, 1 channel, kernel 1, dilation 1, ReLU, not gated,
# head_size 1, head kernel 1, no head bias; layer1x1 active; head1x1 inactive.
# weights in NAMCore order (the order nam-rs's build_array consumes them):
#   rechannel      = 1.0
#   conv_w         = 2.0
#   conv_b         = 0.5
#   mix_w          = 1.0
#   layer1x1_w     = 3.0
#   layer1x1_b     = 0.1
#   head_rechannel = 0.5
#   head_scale     = 10.0
RECHANNEL = 1.0
CONV_W, CONV_B = 2.0, 0.5
MIX_W = 1.0
LAYER1X1_W, LAYER1X1_B = 3.0, 0.1
HEAD_RECHANNEL = 0.5
HEAD_SCALE = 10.0


def relu(x):
    return x if x > 0.0 else 0.0


def wavenet_sample(x):
    """One sample through the tiny WaveNet (mono, condition == input).

    Follows NAMCore's layer order: rechannel -> dilated conv -> input mixer ->
    sum -> activation -> head accumulate -> head rechannel -> head_scale. The
    residual (layer1x1) output is *not* the model output; the head is.
    """
    cur = RECHANNEL * x              # rechannel (1x1, no bias)
    conv_out = CONV_W * cur + CONV_B  # dilated conv, kernel 1, dilation 1
    mix = MIX_W * x                   # input mixer (condition == input)
    z = conv_out + mix
    act = relu(z)                     # NONE gating, primary activation ReLU
    head_accum = act                  # head1x1 inactive; incoming head is silence
    head_out = HEAD_RECHANNEL * head_accum
    # residual would be: x + LAYER1X1_W * act + LAYER1X1_B  (feeds the next array)
    return HEAD_SCALE * head_out


def read_raw(path):
    data = Path(path).read_bytes()
    n = len(data) // 4
    return list(struct.unpack("<%df" % n, data[: n * 4]))


def main():
    outdir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(".")
    inp = read_raw(outdir / "nam_input.raw")
    got = read_raw(outdir / "nam_output.raw")
    if len(inp) != len(got):
        print("FAIL: length mismatch: input %d, output %d" % (len(inp), len(got)))
        return 1

    max_err = 0.0
    sum_sq = 0.0
    ref = []
    for i, x in enumerate(inp):
        y = wavenet_sample(x)
        ref.append(y)
        err = abs(y - got[i])
        max_err = max(max_err, err)
        sum_sq += err * err
    rms_err = math.sqrt(sum_sq / len(inp))

    print("samples: %d" % len(inp))
    print("max abs error: %.3e" % max_err)
    print("rms abs error: %.3e" % rms_err)
    for i in (0, 1, 2, 31, 32, 63, 64, 127, 255, 511):
        if i < len(inp):
            print("  [%3d] in=%+.6f  ref=%+.6f  rust=%+.6f" % (i, inp[i], ref[i], got[i]))

    if max_err > 1e-5:
        print("FAIL: max abs error %.3e exceeds 1e-5" % max_err)
        return 1
    print("PASS: Rust Nam block matches the Python reference within 1e-5")
    return 0


if __name__ == "__main__":
    sys.exit(main())
