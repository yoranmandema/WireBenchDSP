/* c_harness.c — link the real staticlib from C and run the DSP.
 *
 * This is the check that matters for the VST3 shell: it proves the symbols
 * exist with C linkage, the header matches the implementation, and the DSP
 * actually runs and produces a non-silent signal. The output goes to a buffer
 * in memory — nothing is ever played.
 *
 * Build and run with ffi/test-c.sh [preset.json].
 */

#include "pedal_ffi.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

static char *read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) {
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *buf = malloc((size_t)len + 1);
    if (!buf) {
        fclose(f);
        return NULL;
    }
    if (fread(buf, 1, (size_t)len, f) != (size_t)len) {
        free(buf);
        fclose(f);
        return NULL;
    }
    buf[len] = '\0';
    fclose(f);
    return buf;
}

static int g_nodes = 0;
static int g_params = 0;
static float g_first_norm = -1.0f;
static char g_nodes_list[16][128];
static char g_first_node[128] = {0};
static char g_first_param[128] = {0};
static const char *g_current_node = NULL;

static void on_node(void *userdata, const char *node_id) {
    (void)userdata;
    if (g_nodes < (int)(sizeof(g_nodes_list) / sizeof(g_nodes_list[0]))) {
        strncpy(g_nodes_list[g_nodes], node_id, sizeof(g_nodes_list[0]) - 1);
    }
    g_nodes++;
}

static void on_param(void *userdata, const char *param_id, float norm) {
    (void)userdata;
    if (g_params == 0) {
        strncpy(g_first_param, param_id, sizeof(g_first_param) - 1);
        g_first_norm = norm;
        if (g_current_node) {
            strncpy(g_first_node, g_current_node, sizeof(g_first_node) - 1);
        }
    }
    g_params++;
}

int main(int argc, char **argv) {
    const char *preset = argc > 1 ? argv[1] : "presets/04-rainbow.json";
    const uint32_t sr = 48000;
    const uint32_t block = 64;

    PedalCore *core = pedal_core_new((float)sr, block);
    if (!core) {
        printf("FAIL pedal_core_new returned null\n");
        return 1;
    }

    char *json = read_file(preset);
    if (!json) {
        printf("FAIL cannot read preset: %s\n", preset);
        return 1;
    }
    if (pedal_core_load_preset(core, json) != PEDAL_OK) {
        printf("FAIL load_preset: %s\n", pedal_core_last_error(core));
        return 1;
    }

    pedal_core_foreach_node(core, on_node, NULL);
    if (g_nodes == 0) {
        printf("FAIL no nodes enumerated\n");
        return 1;
    }
    /* io.input / io.output have no parameters, so walk to the first node that
     * exposes one. */
    int counted = g_nodes < 16 ? g_nodes : 16;
    for (int i = 0; i < counted; i++) {
        g_params = 0;
        g_first_param[0] = '\0';
        g_current_node = g_nodes_list[i];
        pedal_core_foreach_param(core, g_nodes_list[i], on_param, NULL);
        if (g_params > 0) {
            break;
        }
    }
    if (g_params == 0) {
        printf("FAIL no node exposes a parameter\n");
        return 1;
    }
    printf("preset=%s nodes=%d first_node=%s first_param=%s norm=%.4f latency=%u\n",
           preset, g_nodes, g_first_node, g_first_param, g_first_norm,
           pedal_core_latency(core));

    /* 16 blocks of a 220 Hz sine, stereo interleaved. */
    float in[128];
    float out[128];
    double peak = 0.0;
    double sumsq = 0.0;
    long frames = 0;
    for (int b = 0; b < 16; b++) {
        for (uint32_t i = 0; i < block; i++) {
            float s = 0.2f * sinf(2.0f * (float)M_PI * 220.0f *
                                  (float)(b * (int)block + (int)i) / (float)sr);
            in[2 * i] = s;
            in[2 * i + 1] = s;
        }
        if (pedal_core_process(core, in, out, block, 2) != PEDAL_OK) {
            printf("FAIL process: %s\n", pedal_core_last_error(core));
            return 1;
        }
        for (uint32_t i = 0; i < block; i++) {
            float l = out[2 * i];
            float r = out[2 * i + 1];
            if (l != r) {
                printf("FAIL mono->stereo mismatch at frame %u\n", i);
                return 1;
            }
            double a = fabs((double)l);
            if (a > peak) {
                peak = a;
            }
            sumsq += (double)l * (double)l;
            frames++;
        }
    }
    double rms = sqrt(sumsq / (double)frames);
    printf("peak=%.6f rms=%.6f frames=%ld\n", peak, rms, frames);
    if (!(peak > 0.01 && rms > 0.001)) {
        printf("FAIL output is silent\n");
        return 1;
    }

    /* Parameter round trip through the C ABI. */
    float norm = -1.0f;
    if (pedal_core_get_param_norm(core, g_first_node, g_first_param, &norm) != PEDAL_OK) {
        printf("FAIL get_param_norm\n");
        return 1;
    }
    if (pedal_core_set_param_norm(core, g_first_node, g_first_param, 0.25f) != PEDAL_OK) {
        printf("FAIL set_param_norm\n");
        return 1;
    }
    float after = -1.0f;
    if (pedal_core_get_param_norm(core, g_first_node, g_first_param, &after) != PEDAL_OK) {
        printf("FAIL get_param_norm after set\n");
        return 1;
    }
    printf("param %.4f -> 0.2500 -> %.4f\n", norm, after);
    if (fabsf(after - 0.25f) > 1e-5f) {
        printf("FAIL parameter did not stick\n");
        return 1;
    }

    /* Error path: a null core must not crash. */
    pedal_core_free(NULL);
    if (pedal_core_last_error(NULL) == NULL) {
        printf("FAIL last_error(NULL) returned null\n");
        return 1;
    }

    pedal_core_free(core);
    free(json);
    printf("PASS\n");
    return 0;
}
