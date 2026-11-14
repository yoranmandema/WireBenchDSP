# pedal-ffi

The C ABI between the Rust DSP core and the native plugin shell (iPlug2, C++).

The VST3 plugin cannot call Rust directly, so this crate exposes the `dsp-graph`
executor through a flat C boundary. The shell links the `staticlib` and includes
`include/pedal_ffi.h`.

```
ffi/
  src/lib.rs          the boundary
  include/pedal_ffi.h the header the C++ shell includes
  tests/abi.rs        the same extern block, driven from Rust
  tests/c_harness.c   a real C program linked against the staticlib
  test-c.sh           builds the staticlib and runs the C harness
```

## Contract

| Function | Purpose |
|---|---|
| `pedal_core_new(sr, max_block)` | create a core; `max_block` is the largest block the host will ever request |
| `pedal_core_load_preset(core, json)` / `_file` | parse, build, and compile a rig |
| `pedal_core_set_sample_rate` | change rate; recompiles the loaded preset |
| `pedal_core_set_param_norm` / `_real` | parameters, normalised 0..1 (VST3 automation) or in the block's own unit |
| `pedal_core_get_param_norm` / `_real` | read back |
| `pedal_core_set_bypass` | bypass a node |
| `pedal_core_process(core, in, out, n, channels)` | run one block |
| `pedal_core_reset` | clear all node state |
| `pedal_core_latency` | total algorithmic latency, in samples |
| `pedal_core_foreach_node` / `_param` | enumerate for a native UI |

Conventions:

- **Parameters are normalised 0..1** on the wire, matching VST3 automation.
  `*_real` exists for preset loading and for hosts that prefer real units.
- **The graph is mono.** `io.input` / `io.output` have one audio port, so a
  stereo host passes interleaved stereo and gets the mono signal duplicated to
  both channels. True stereo is a later story.
- **`process` never allocates.** Stereo deinterleave uses a scratch buffer
  allocated once in `pedal_core_new`.
- **No panics cross the boundary.** Every entry point checks its pointers;
  failures return `PEDAL_ERR` and set `pedal_core_last_error`.
- A block larger than the declared `max_block` is refused rather than clamped,
  so a host never reads unwritten output.

## Verification

```sh
cargo test -p pedal-ffi          # the ABI driven from Rust
ffi/test-c.sh                    # the ABI linked from C, DSP runs, output checked
```

The C harness renders a 220 Hz sine through a real preset and asserts a
non-silent output. It writes to memory only — it never opens an audio device,
so it is safe to run on a machine with attached speakers.

## Not in this crate

The C++ shell, the VST3 wrapper, and the plugin's own UI. Those live in
`plugin/` and link `libpedal_ffi.a`.
