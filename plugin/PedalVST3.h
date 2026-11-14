#pragma once

#include "IPlug_include_in_plug_hdr.h"

const int kNumPresets = 1;

enum EParams
{
  kAmpGain = 0,
  kAmpLevel,
  kDelayTime,
  kFeedback,
  kPitch,
  kSweepRate,
  kMix,
  kMasterVol,
  kNumParams
};

using namespace iplug;
using namespace igraphics;

typedef struct PedalCore PedalCore;

class PedalVST3 final : public Plugin
{
public:
  PedalVST3(const InstanceInfo& info);
  ~PedalVST3() override;

#if IPLUG_DSP // http://bit.ly/2S64BDd
  void ProcessBlock(sample** inputs, sample** outputs, int nFrames) override;
  void OnReset() override;
#endif

private:
  PedalCore* mCore = nullptr;
  double mSampleRate = 48000.0;
  bool mPresetLoaded = false;
};
