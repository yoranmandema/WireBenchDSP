#pragma once

/* config.h — iPlug2 project configuration for the Pedal DSP VST3 shell.
 *
 * Only the VST3 format is built (see CMakeLists.txt: FORMATS VST3), so the
 * AUv2 / AUv3 / AAX / CLAP blocks from the iPlug2 examples are omitted.
 */

#define PLUG_NAME "Pedal DSP"
#define PLUG_MFR "Pedal DSP"
#define PLUG_VERSION_HEX 0x00010000
#define PLUG_VERSION_STR "1.0.0"
#define PLUG_UNIQUE_ID 'PDSP'
#define PLUG_MFR_ID 'PDSP'
#define PLUG_URL_STR "https://github.com/morluto/pedal-dsp"
#define PLUG_EMAIL_STR ""
#define PLUG_COPYRIGHT_STR "Copyright 2026 Pedal DSP"
#define PLUG_CLASS_NAME PedalVST3

#define BUNDLE_NAME "PedalVST3"
#define BUNDLE_MFR "PedalDSP"
#define BUNDLE_DOMAIN "com"

#define SHARED_RESOURCES_SUBPATH "PedalVST3"

/* The graph is mono; a stereo host gets the mono signal on both channels
 * (see PedalVST3::ProcessBlock). */
#define PLUG_CHANNEL_IO "1-1 2-2"

#define PLUG_LATENCY 0
#define PLUG_TYPE 0
#define PLUG_DOES_MIDI_IN 0
#define PLUG_DOES_MIDI_OUT 0
#define PLUG_DOES_MPE 0
#define PLUG_DOES_STATE_CHUNKS 0
#define PLUG_HAS_UI 1
#define PLUG_WIDTH 800
#define PLUG_HEIGHT 300
#define PLUG_FPS 60
#define PLUG_SHARED_RESOURCES 0
#define PLUG_HOST_RESIZE 0

#define VST3_SUBCATEGORY "Fx"

#define APP_NUM_CHANNELS 2
#define APP_SIGNAL_VECTOR_SIZE 64

#define ROBOTO_FN "Roboto-Regular.ttf"
