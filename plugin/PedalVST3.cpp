#include "PedalVST3.h"
#include "IPlug_include_in_plug_src.h"
#include "IControls.h"

#include <cstring>

#include "pedal_ffi.h"

namespace {

/* The largest block the host may ask for. `pedal_core_process` refuses
 * anything bigger, so this must cover every host's maximum. */
constexpr uint32_t kMaxBlock = 4096;

/* The preset loaded when the plugin starts. Kept in the bundle's Resources
 * (see RESOURCES in CMakeLists.txt); PEDAL_FALLBACK_PRESET is the source tree
 * copy, so a build directory sitting next to the repo still loads. */
constexpr const char* kDefaultPreset = "04-rainbow.json";

/* One VST3 parameter per control the UI exposes. The ranges/defaults mirror
 * the block specs in graph/src/blocks.rs, so automation lands on the same
 * real values the UI would send; the C ABI takes real values. */
struct ParamMap {
  int index;
  const char* node;
  const char* param;
  const char* name;
  double def;
  double min;
  double max;
  double step;
};

const ParamMap kParamMap[kNumParams] = {
  { kAmpGain,   "amp",    "gain",     "Amp Gain",   3.5,    0.0,   10.0,   0.01 },
  { kAmpLevel,  "amp",    "level",    "Amp Level", -5.0,  -40.0,   12.0,   0.1  },
  { kDelayTime, "pitch",  "time",     "Delay Time", 380.0,  20.0, 1000.0,  1.0  },
  { kFeedback,  "pitch",  "feedback", "Feedback",   60.0,   0.0,   95.0,   0.5  },
  { kPitch,     "pitch",  "pitch",    "Pitch",      700.0, -1200.0, 1200.0, 1.0 },
  { kSweepRate, "pitch",  "rate",     "Sweep Rate", 0.0,  -1200.0, 1200.0, 1.0 },
  { kMix,       "pitch",  "mix",      "Mix",        45.0,   0.0,  100.0,   0.5  },
  { kMasterVol, "master", "volume",   "Master",     0.0,   -60.0,   12.0,   0.1  },
};

WDL_String PresetPath()
{
  WDL_String path;
  BundleResourcePath(path, BUNDLE_ID);

  if (path.GetLength())
    path.Append("/presets/");

  path.Append(kDefaultPreset);
  return path;
}

} // namespace

PedalVST3::PedalVST3(const InstanceInfo& info)
: iplug::Plugin(info, MakeConfig(kNumParams, kNumPresets))
{
  for (const ParamMap& pm : kParamMap)
    GetParam(pm.index)->InitDouble(pm.name, pm.def, pm.min, pm.max, pm.step);

#if IPLUG_EDITOR // http://bit.ly/2S64BDD
  mMakeGraphicsFunc = [&]() {
    return MakeGraphics(*this, PLUG_WIDTH, PLUG_HEIGHT, PLUG_FPS, GetScaleForScreen(PLUG_WIDTH, PLUG_HEIGHT));
  };

  mLayoutFunc = [&](IGraphics* pGraphics) {
    pGraphics->AttachCornerResizer(EUIResizerMode::Scale, false);
    pGraphics->AttachPanelBackground(COLOR_GRAY);
    pGraphics->LoadFont("Roboto-Regular", ROBOTO_FN);

    const IRECT bounds = pGraphics->GetBounds().GetPadded(-10.f);
    const IRECT header = bounds.GetFromTop(40.f);
    const IRECT knobs = bounds.GetReducedFromTop(40.f).GetPadded(-5.f);

    WDL_String buildInfoStr;
    GetBuildInfoStr(buildInfoStr, __DATE__, __TIME__);
    pGraphics->AttachControl(new ITextControl(header.GetFromTLHC(bounds.W(), 40.f),
                                              PLUG_NAME "  " PLUG_VERSION_STR,
                                              IText(24.f)));
    pGraphics->AttachControl(new ITextControl(header.GetFromTRHC(300.f, 20.f),
                                              buildInfoStr.Get(),
                                              DEFAULT_TEXT.WithAlign(EAlign::Far).WithSize(12.f)));

    for (int i = 0; i < kNumParams; i++)
    {
      const IRECT cell = knobs.GetGridCell(0, i, 1, kNumParams).GetPadded(-4.f);
      pGraphics->AttachControl(new IVKnobControl(cell, kParamMap[i].index, kParamMap[i].name));
    }
  };
#endif
}

PedalVST3::~PedalVST3()
{
  pedal_core_free(mCore);
  mCore = nullptr;
}

#if IPLUG_DSP

void PedalVST3::OnReset()
{
  const double sr = GetSampleRate();

  if (!mCore)
    mCore = pedal_core_new((float) sr, kMaxBlock);
  else if (sr != mSampleRate)
    pedal_core_set_sample_rate(mCore, (float) sr);

  mSampleRate = sr;

  /* Stereo in and out, plus slack for the mono case. */
  mIO.assign((size_t) kMaxBlock * 2, 0.f);

  if (!mPresetLoaded)
  {
    const WDL_String path = PresetPath();
    int rc = pedal_core_load_preset_file(mCore, path.Get());

    if (rc != PEDAL_OK)
    {
#if defined(PEDAL_FALLBACK_PRESET)
      rc = pedal_core_load_preset_file(mCore, PEDAL_FALLBACK_PRESET);
#endif
    }

    if (rc == PEDAL_OK)
    {
      mPresetLoaded = true;
      SetLatency((int) pedal_core_latency(mCore));
    }
    else
    {
      // Leave the plugin silent rather than reporting failure on the audio
      // thread; the message is retrievable through the C ABI if a host wants it.
      mPresetLoaded = false;
    }
  }
}

void PedalVST3::ProcessBlock(sample** inputs, sample** outputs, int nFrames)
{
  if (!mPresetLoaded)
    OnReset();

  const int nIn = NInChansConnected();
  const int nOut = NOutChansConnected();

  if (!mCore || !mPresetLoaded)
  {
    for (int c = 0; c < nOut; c++)
      std::memset(outputs[c], 0, (size_t) nFrames * sizeof(sample));
    return;
  }

  /* Real values cross the C ABI, not normalised ones: iPlug2's parameter
   * shape is linear while several block specs use log / amp-gain curves
   * (delay time, amp level, master volume), and normalising linearly would
   * land on the wrong real value. */
  for (const ParamMap& pm : kParamMap)
    pedal_core_set_param_real(mCore, pm.node, pm.param,
                              (float) GetParam(pm.index)->Value());

  const int chans = (nIn >= 2 && nOut >= 2) ? 2 : 1;

  /* double (iPlug2) -> interleaved float (C ABI) */
  if (chans == 2)
  {
    for (int s = 0; s < nFrames; s++)
    {
      mIO[(size_t) s * 2]     = (float) inputs[0][s];
      mIO[(size_t) s * 2 + 1] = (float) inputs[1][s];
    }
  }
  else
  {
    for (int s = 0; s < nFrames; s++)
      mIO[s] = (float) inputs[0][s];
  }

  pedal_core_process(mCore, mIO.data(), mIO.data(), (uint32_t) nFrames, (uint32_t) chans);

  /* interleaved float -> double, per host channel */
  if (chans == 2)
  {
    for (int s = 0; s < nFrames; s++)
    {
      outputs[0][s] = (sample) mIO[(size_t) s * 2];
      outputs[1][s] = (sample) mIO[(size_t) s * 2 + 1];
    }
  }
  else
  {
    for (int s = 0; s < nFrames; s++)
      outputs[0][s] = (sample) mIO[s];
  }

  /* If the host gave us more outputs than the graph has channels, mute the rest. */
  for (int c = chans; c < nOut; c++)
    std::memset(outputs[c], 0, (size_t) nFrames * sizeof(sample));
}

#endif
