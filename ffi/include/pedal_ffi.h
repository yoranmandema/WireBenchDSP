/* pedal_ffi.h — C ABI between the Rust DSP core and the native plugin shell.
 *
 * The VST3 shell (iPlug2, C++) links the `libpedal_ffi.a` staticlib and talks to
 * the DSP core through these functions only. See ffi/src/lib.rs for the Rust
 * side and ffi/README.md for the contract.
 *
 * Conventions:
 *   - Parameters are normalised 0..1 on the wire, like VST3 automation.
 *   - Audio is mono inside the graph. A stereo host passes interleaved stereo
 *     and gets the mono signal duplicated to both channels.
 *   - `pedal_core_process` never allocates.
 *   - Every fallible call returns PEDAL_OK (0) or PEDAL_ERR (-1); on error,
 *     `pedal_core_last_error` carries a message.
 */
#ifndef PEDAL_FFI_H
#define PEDAL_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PedalCore PedalCore;

#define PEDAL_OK 0
#define PEDAL_ERR (-1)

/* Create / destroy. `max_block` is the largest block the host will ever ask
 * for; `process` refuses anything larger. */
PedalCore *pedal_core_new(float sample_rate, uint32_t max_block);
void pedal_core_free(PedalCore *core);
const char *pedal_core_last_error(const PedalCore *core);

/* Presets. `load_preset` takes JSON text; `load_preset_file` reads it first.
 * Both replace any rig already loaded. */
int pedal_core_load_preset(PedalCore *core, const char *json);
int pedal_core_load_preset_file(PedalCore *core, const char *path);

/* Change the host sample rate; recompiles the loaded preset. */
int pedal_core_set_sample_rate(PedalCore *core, float sample_rate);

/* Parameters. `*_norm` take and return 0..1; `*_real` take and return the
 * parameter's own unit (dB, Hz, ms, ...), denormalised by the block spec. */
int pedal_core_set_param_norm(PedalCore *core, const char *node,
                              const char *param, float norm);
int pedal_core_set_param_real(PedalCore *core, const char *node,
                              const char *param, float real);
int pedal_core_get_param_norm(PedalCore *core, const char *node,
                              const char *param, float *out);
int pedal_core_get_param_real(PedalCore *core, const char *node,
                              const char *param, float *out);

/* Bypass a node: its first input is copied to every output. */
int pedal_core_set_bypass(PedalCore *core, const char *node, int on);

/* Process one block. `input` and `output` are interleaved, `n` frames each.
 * `channels` is 1 or 2. Returns PEDAL_ERR if no preset is loaded or `n`
 * exceeds the configured maximum. */
int pedal_core_process(PedalCore *core, const float *input, float *output,
                       uint32_t n, uint32_t channels);

/* Clear all node and delay state (preset change, bypass). */
void pedal_core_reset(PedalCore *core);

/* The graph's total algorithmic latency in samples. */
uint32_t pedal_core_latency(const PedalCore *core);

/* Enumeration, for a native plugin UI. The callback pointers point into the
 * core and stay valid for the duration of the call only. */
typedef void (*PedalNodeCb)(void *userdata, const char *node_id);
typedef void (*PedalParamCb)(void *userdata, const char *param_id, float norm);
void pedal_core_foreach_node(PedalCore *core, PedalNodeCb cb, void *userdata);
void pedal_core_foreach_param(PedalCore *core, const char *node,
                              PedalParamCb cb, void *userdata);

#ifdef __cplusplus
}
#endif

#endif /* PEDAL_FFI_H */
