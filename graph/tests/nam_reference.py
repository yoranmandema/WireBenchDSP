#!/usr/bin/env python3
"""Self-contained Python reference render of a NAM WaveNet.

The Neural Amp Modeler Python package is not available on this host (the PyPI
`nam` package is a different project — Google's Neural Additive Models — and
NAM's own `nam` needs torch, which is not installed). So this script implements
the same WaveNet forward pass in pure Python and compares it sample by sample
against the Rust `Nam` block's output.

Two modes:

* **tiny model** (default): the A1-style WaveNet copied from
  `graph/tests/nam.rs::ir_model()`, compared against the dump from
  `graph/examples/nam_reference.rs`.

      cargo run -p dsp-graph --example nam_reference -- <outdir>
      python3 graph/tests/nam_reference.py <outdir>

* **real `.nam`** (`--model PATH`): the `.nam` is parsed here and its weights
  drive the same forward pass, compared against the dump from
  `graph/examples/nam_real_reference.rs`. A `SlimmableContainer` runs its
  default (full) submodel, matching `nam-rs`'s `active = len - 1`.

      cargo run -p dsp-graph --example nam_real_reference -- <model.nam> <outdir>
      python3 graph/tests/nam_reference.py --model <model.nam> <outdir>

Both modes read `<outdir>/nam_input.raw` and `<outdir>/nam_output.raw`
(little-endian f32) and exit non-zero if any sample differs by more than the
tolerance (1e-5 by default; `--tol` overrides).

The generic path supports the WaveNet features the shipped models use: one or
more layer-arrays, per-layer kernel/dilation/activation, the three gating modes,
`layer1x1` / `head1x1`, and a multi-tap head rechannel. Grouped convolutions,
FiLM, post-stack heads, and `condition_dsp` are rejected with a clear error
rather than silently mis-ran.
"""

import argparse
import json
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


# --- generic .nam parser + forward pass --------------------------------------

class NamUnsupported(Exception):
    """A config feature this reference does not implement."""


def _sigmoid(x):
    return 1.0 / (1.0 + math.exp(-x))


def _activation(spec):
    """Resolve a NAM activation spec (string or object) to a callable."""
    if isinstance(spec, str):
        name, slope = spec, None
    elif isinstance(spec, dict):
        name = spec.get("type")
        slope = spec.get("negative_slope")
    else:
        raise NamUnsupported(f"unsupported activation: {spec!r}")
    if name == "Tanh":
        return math.tanh
    if name == "ReLU":
        return lambda x: x if x > 0.0 else 0.0
    if name == "Sigmoid":
        return _sigmoid
    if name == "LeakyReLU":
        s = 0.01 if slope is None else float(slope)
        return lambda x, s=s: x if x > 0.0 else s * x
    raise NamUnsupported(f"unsupported activation: {name!r}")


def _gating_mode(spec):
    name = spec if isinstance(spec, str) else spec.get("type")
    if name not in ("none", "gated", "blended"):
        raise NamUnsupported(f"unsupported gating_mode: {spec!r}")
    return name


def _broadcast(value, n, parse, kind):
    """Expand a scalar (or per-layer list) to `n` entries, like nam-rs."""
    if isinstance(value, list):
        if len(value) != n:
            raise NamUnsupported(f"{kind}: {len(value)} entries for {n} layers")
        return [parse(v) for v in value]
    return [parse(value)] * n


def _conv_block(weights, bias, x, in_ch, out_ch, kernel, dilation):
    """Causal dilated convolution over a whole signal, `groups = 1`.

    `x` is `in_ch` rows of `N` floats; `weights` is `[out][in][kernel]`
    (NAM/PyTorch Conv1d order, tap 0 = oldest). Returns `out_ch` rows of `N`
    floats. Summation order matches nam-rs's block kernel (o, then tap, then
    input channel) so the two implementations agree to float rounding.
    """
    n = len(x[0])
    out = [[bias[o] if bias is not None else 0.0] * n for o in range(out_ch)]
    for o in range(out_ch):
        dst = out[o]
        wo = o * in_ch * kernel
        for k in range(kernel):
            back = (kernel - 1 - k) * dilation
            for j in range(in_ch):
                w = weights[wo + j * kernel + k]
                if w == 0.0:
                    continue
                src = x[j]
                dst[back:] = [a + w * b for a, b in zip(dst[back:], src[: n - back])]
    return out


def _gating_rows(z, bn, mode, primary, secondary):
    """Apply the layer's gating over `mid` rows, returning `bn` rows."""
    if mode == "none":
        return [[primary(v) for v in z[c]] for c in range(bn)]
    out = []
    for c in range(bn):
        vrow, grow = z[c], z[c + bn]
        if mode == "gated":
            out.append([primary(v) * secondary(g) for v, g in zip(vrow, grow)])
        else:  # blended
            row = []
            for v, g in zip(vrow, grow):
                alpha = secondary(g)
                row.append(alpha * primary(v) + (1.0 - alpha) * v)
            out.append(row)
    return out


class _Layer:
    def __init__(self, channels, bottleneck, condition_size, kernel, dilation,
                 mode, primary, secondary, conv_w, conv_b, mix_w,
                 layer1x1_w, layer1x1_b, head1x1_w, head1x1_b, head1x1_out):
        self.channels = channels
        self.bottleneck = bottleneck
        self.condition_size = condition_size
        self.kernel = kernel
        self.dilation = dilation
        self.mode = mode
        self.primary = primary
        self.secondary = secondary
        self.conv_w = conv_w
        self.conv_b = conv_b
        self.mix_w = mix_w
        self.layer1x1_w = layer1x1_w
        self.layer1x1_b = layer1x1_b
        self.head1x1_w = head1x1_w
        self.head1x1_b = head1x1_b
        self.head1x1_out = head1x1_out
        self.mid = bottleneck * 2 if mode != "none" else bottleneck
        # Width of the head contribution: head1x1 output when active, else the
        # bottleneck (nam-rs `Layer::head_contrib_width`).
        self.head_contrib_width = head1x1_out if head1x1_w is not None else bottleneck

    def process(self, x, cond, head):
        conv_out = _conv_block(self.conv_w, self.conv_b, x, self.channels,
                               self.mid, self.kernel, self.dilation)
        mix = _conv_block(self.mix_w, None, cond, self.condition_size,
                          self.mid, 1, 1)
        z = [[c + m for c, m in zip(cr, mr)] for cr, mr in zip(conv_out, mix)]
        act = _gating_rows(z, self.bottleneck, self.mode, self.primary, self.secondary)
        if self.head1x1_w is not None:
            contrib = _conv_block(self.head1x1_w, self.head1x1_b, act,
                                  self.bottleneck, self.head1x1_out, 1, 1)
        else:
            contrib = act
        head = [[h + c for h, c in zip(hr, cr)] for hr, cr in zip(head, contrib)]
        if self.layer1x1_w is not None:
            res = _conv_block(self.layer1x1_w, self.layer1x1_b, act,
                              self.bottleneck, self.channels, 1, 1)
            out = [[x + r for x, r in zip(xr, rr)] for xr, rr in zip(x, res)]
        else:
            out = x
        return out, head


class _Array:
    def __init__(self, input_size, channels, rechannel_w, layers,
                 head_w, head_b, head_in, head_size, head_kernel_size):
        self.input_size = input_size
        self.channels = channels
        self.rechannel_w = rechannel_w
        self.layers = layers
        self.head_w = head_w
        self.head_b = head_b
        self.head_in = head_in
        self.head_size = head_size
        self.head_kernel_size = head_kernel_size

    def process(self, inp, cond, head_prev):
        cur = _conv_block(self.rechannel_w, None, inp, self.input_size,
                          self.channels, 1, 1)
        head = [list(h) for h in head_prev]
        for layer in self.layers:
            cur, head = layer.process(cur, cond, head)
        head_out = _conv_block(self.head_w, self.head_b, head, self.head_in,
                               self.head_size, self.head_kernel_size, 1)
        return head_out, cur


class NamWaveNet:
    """A parsed `.nam` (WaveNet or SlimmableContainer) and its forward pass."""

    def __init__(self, nam, active_index=None):
        arch = nam.get("architecture")
        self.active_index = None
        if arch == "SlimmableContainer":
            subs = nam["config"]["submodels"]
            if not subs:
                raise NamUnsupported("empty SlimmableContainer")
            self.active_index = len(subs) - 1 if active_index is None else active_index
            nam = subs[self.active_index]["model"]
            arch = nam.get("architecture")
        if arch != "WaveNet":
            raise NamUnsupported(f"unsupported architecture: {arch!r}")

        cfg = nam["config"]
        if cfg.get("head") is not None:
            raise NamUnsupported("post-stack head not supported by the reference")
        if cfg.get("condition_dsp") is not None:
            raise NamUnsupported("condition_dsp not supported by the reference")

        weights = nam["weights"]
        pos = 0

        def take(k):
            nonlocal pos
            if pos + k > len(weights):
                raise NamUnsupported(
                    f"weight blob too short: need {k} at {pos}, have {len(weights)}")
            v = weights[pos:pos + k]
            pos += k
            return v

        self.arrays = [self._build_array(la, take) for la in cfg["layers"]]
        self.head_scale = take(1)[0]
        if pos != len(weights):
            raise NamUnsupported(
                f"consumed {pos} of {len(weights)} weights — parser/weight-layout mismatch")

    @staticmethod
    def _build_array(la, take):
        n = len(la["dilations"])
        input_size = la["input_size"]
        condition_size = la["condition_size"]
        channels = la["channels"]
        bottleneck = la.get("bottleneck", channels)
        if "kernel_sizes" in la:
            kernel_sizes = list(la["kernel_sizes"])
        else:
            kernel_sizes = [la["kernel_size"]] * n
        dilations = list(la["dilations"])
        activations = _broadcast(la["activation"], n, _activation, "activation")
        modes = _broadcast(la.get("gating_mode", "none"), n, _gating_mode, "gating_mode")
        secondary = _broadcast(
            la.get("secondary_activation"), n,
            lambda v: _sigmoid if v is None else _activation(v),
            "secondary_activation")
        mode = modes[0]
        if any(m != mode for m in modes):
            raise NamUnsupported("mixed gating modes")

        h = la.get("head")
        if h is not None:
            head_size = h["out_channels"]
            head_kernel = h["kernel_size"]
            head_bias = h.get("bias", True)
        else:
            head_size = la["head_size"]
            head_kernel = 1
            head_bias = la.get("head_bias", False)

        l1 = la.get("layer1x1") or {}
        l1_active = l1.get("active", False)
        l1_groups = l1.get("groups", 1)
        h1 = la.get("head1x1") or {}
        h1_active = h1.get("active", False)
        h1_out = h1.get("out_channels", channels)
        h1_groups = h1.get("groups", 1)
        groups_input = la.get("groups_input", 1)
        groups_input_mixin = la.get("groups_input_mixin", 1)
        if groups_input != 1 or groups_input_mixin != 1 or l1_groups != 1 or h1_groups != 1:
            raise NamUnsupported("grouped convolutions not supported by the reference")

        for key in ("conv_pre_film", "conv_post_film", "input_mixin_pre_film",
                    "input_mixin_post_film", "activation_pre_film",
                    "activation_post_film", "layer1x1_post_film", "head1x1_post_film"):
            f = la.get(key)
            if f and f.get("active"):
                raise NamUnsupported(f"{key} not supported by the reference")

        mid = bottleneck * 2 if mode != "none" else bottleneck
        head1x1_out = h1_out if h1_active else channels

        rechannel_w = take(channels * input_size)
        layers = []
        for i in range(n):
            conv_w = take(mid * channels * kernel_sizes[i] // groups_input)
            conv_b = take(mid)
            mix_w = take(mid * condition_size // groups_input_mixin)
            l1_w = take(channels * bottleneck // l1_groups) if l1_active else None
            l1_b = take(channels) if l1_active else None
            h1_w = take(head1x1_out * bottleneck // h1_groups) if h1_active else None
            h1_b = take(head1x1_out) if h1_active else None
            layers.append(_Layer(
                channels, bottleneck, condition_size, kernel_sizes[i],
                dilations[i], mode, activations[i], secondary[i],
                conv_w, conv_b, mix_w, l1_w, l1_b, h1_w, h1_b, head1x1_out))

        head_in = layers[0].head_contrib_width
        head_w = take(head_size * head_in * head_kernel)
        head_b = take(head_size) if head_bias else None
        return _Array(input_size, channels, rechannel_w, layers,
                      head_w, head_b, head_in, head_size, head_kernel)

    def process(self, xs):
        """Run a mono signal through the model; returns a list of N floats."""
        n = len(xs)
        if self.arrays[0].input_size != 1:
            raise NamUnsupported("multi-channel input not supported")
        if self.arrays[0].layers[0].condition_size != 1:
            raise NamUnsupported("condition_size != 1 (no condition_dsp) not supported")
        cond = [list(xs)]
        head = [[0.0] * n for _ in range(self.arrays[0].head_in)]
        head_out, sig = self.arrays[0].process([list(xs)], cond, head)
        for arr in self.arrays[1:]:
            head_out, sig = arr.process(sig, cond, head_out)
        # No post-stack head: output = head_scale * final head (head_size 1).
        return [self.head_scale * v for v in head_out[0]]


# --- comparison --------------------------------------------------------------

def read_raw(path):
    data = Path(path).read_bytes()
    n = len(data) // 4
    return list(struct.unpack("<%df" % n, data[: n * 4]))


def main():
    ap = argparse.ArgumentParser(description="Python reference render of a NAM WaveNet")
    ap.add_argument("outdir", help="directory holding nam_input.raw / nam_output.raw")
    ap.add_argument("--model", help="real .nam to parse and run (default: tiny model)")
    ap.add_argument("--tol", type=float, default=1e-5, help="max abs error (default 1e-5)")
    args = ap.parse_args()

    outdir = Path(args.outdir)
    inp = read_raw(outdir / "nam_input.raw")
    got = read_raw(outdir / "nam_output.raw")
    if len(inp) != len(got):
        print("FAIL: length mismatch: input %d, output %d" % (len(inp), len(got)))
        return 1

    if args.model:
        nam = json.loads(Path(args.model).read_text())
        net = NamWaveNet(nam)
        ref = net.process(inp)
        label = "%s (submodel %s)" % (args.model, net.active_index)
    else:
        ref = [wavenet_sample(x) for x in inp]
        label = "tiny WaveNet"

    max_err = 0.0
    sum_sq = 0.0
    for i in range(len(inp)):
        err = abs(ref[i] - got[i])
        max_err = max(max_err, err)
        sum_sq += err * err
    rms_err = math.sqrt(sum_sq / len(inp))

    print("model: %s" % label)
    print("samples: %d" % len(inp))
    print("max abs error: %.3e" % max_err)
    print("rms abs error: %.3e" % rms_err)
    for i in (0, 1, 2, 31, 32, 63, 64, 127, 255, 511):
        if i < len(inp):
            print("  [%3d] in=%+.6f  ref=%+.6f  rust=%+.6f" % (i, inp[i], ref[i], got[i]))

    if max_err > args.tol:
        print("FAIL: max abs error %.3e exceeds %.1e" % (max_err, args.tol))
        return 1
    print("PASS: Rust Nam block matches the Python reference within %.1e" % args.tol)
    return 0


if __name__ == "__main__":
    sys.exit(main())
