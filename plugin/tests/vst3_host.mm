// vst3_host.cpp — a minimal VST3 host, used only for headless verification.
//
//   vst3_host <path/to/Plugin.vst3> [out.raw] [seconds]
//
// It loads the bundle through GetPluginFactory, instantiates the component,
// runs a 220 Hz sine through it at 48 kHz and writes the raw stereo float
// output to disk. It never opens an audio device and never plays anything, so
// it is safe on a machine with attached speakers.
//
// This is the "small host that discards output" the worklog asks for: it
// proves the bundle loads in a real VST3 host and that the DSP produces
// audio, without launching a DAW.

#include <dlfcn.h>

#import <Foundation/Foundation.h>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

#include "pluginterfaces/base/funknown.h"
#include "pluginterfaces/base/ipluginbase.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstprocesscontext.h"
#include "public.sdk/source/main/pluginfactory.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/utility/stringconvert.h"

using namespace Steinberg;
using namespace Steinberg::Vst;

static const int32_t kBlock = 512;
static const double kRate = 48000.0;

#define CHECK(x, msg) \
  do { if (!(x)) { std::fprintf(stderr, "FAIL: %s\n", msg); return 1; } } while (0)

int main(int argc, char** argv)
{
  const char* path = argc > 1 ? argv[1] : "PedalVST3.vst3";
  const char* outPath = argc > 2 ? argv[2] : "/tmp/pedal-vst3-out.raw";
  const double seconds = argc > 3 ? std::atof(argv[3]) : 1.0;

  // Load the bundle through NSBundle, so [NSBundle bundleWithIdentifier:]
  // resolves it and the plugin's own resource lookup (BundleResourcePath)
  // works exactly as it does in a real host.
  NSBundle* bundle = [NSBundle bundleWithPath:[NSString stringWithUTF8String:path]];
  CHECK(bundle != nullptr, (std::string("not a loadable bundle: ") + path).c_str());
  CHECK([bundle load], "NSBundle load failed");

  const char* execPath = [[bundle executablePath] UTF8String];
  void* module = dlopen(execPath, RTLD_NOW | RTLD_GLOBAL);
  CHECK(module != nullptr, (std::string("dlopen ") + execPath + ": " + dlerror()).c_str());

  auto getFactory = reinterpret_cast<IPluginFactory* (*)()>(dlsym(module, "GetPluginFactory"));
  CHECK(getFactory != nullptr, "bundle exports no GetPluginFactory");

  IPluginFactory* factory = getFactory();
  CHECK(factory != nullptr, "GetPluginFactory returned null");

  const int32_t nClasses = factory->countClasses();
  std::printf("factory: %d class(es)\n", nClasses);
  CHECK(nClasses >= 1, "factory has no classes");

  PClassInfo info;
  CHECK(factory->getClassInfo(0, &info) == kResultTrue, "getClassInfo(0)");
  std::printf("class 0: %s\n", info.name);

  FUID cid = FUID::fromTUID(info.cid);

  IPtr<IComponent> comp;
  CHECK(factory->createInstance(cid, IComponent::iid, (void**) &comp) == kResultTrue,
        "createInstance(IComponent)");
  CHECK(comp, "component is null");

  HostApplication host;
  CHECK(comp->initialize(&host) == kResultTrue, "IComponent::initialize");

  BusInfo inBus {};
  BusInfo outBus {};
  CHECK(comp->getBusInfo(kAudio, kInput, 0, inBus) == kResultTrue, "getBusInfo(input)");
  CHECK(comp->getBusInfo(kAudio, kOutput, 0, outBus) == kResultTrue, "getBusInfo(output)");
  std::printf("buses: in %d ch, out %d ch\n", inBus.channelCount, outBus.channelCount);

  const int32_t nChans = outBus.channelCount > 0 ? outBus.channelCount : 2;

  CHECK(comp->activateBus(kAudio, kInput, 0, 1) == kResultTrue, "activateBus(input)");
  CHECK(comp->activateBus(kAudio, kOutput, 0, 1) == kResultTrue, "activateBus(output)");

  IPtr<IAudioProcessor> proc;
  CHECK(comp->queryInterface(IAudioProcessor::iid, (void**) &proc) == kResultTrue,
        "queryInterface(IAudioProcessor)");

  ProcessSetup setup {};
  setup.processMode = kRealtime;
  setup.symbolicSampleSize = kSample32;
  setup.maxSamplesPerBlock = kBlock;
  setup.sampleRate = kRate;
  CHECK(proc->setupProcessing(setup) == kResultTrue, "setupProcessing");

  IPtr<IEditController> ctrl;
  if (comp->queryInterface(IEditController::iid, (void**) &ctrl) == kResultTrue && ctrl)
  {
    // SingleComponentEffect: the controller is the same object, already
    // initialized above, so do not call initialize() a second time.
    const int32_t nParams = ctrl->getParameterCount();
    std::printf("parameters: %d\n", nParams);
    for (int32_t i = 0; i < nParams; i++)
    {
      ParameterInfo pi {};
      if (ctrl->getParameterInfo(i, pi) == kResultTrue)
        std::printf("  [%d] %s\n", i, Vst::StringConvert::convert(pi.title).c_str());
    }
  }

  CHECK(comp->setActive(true) == kResultTrue, "setActive(true)");
  CHECK(proc->setProcessing(true) == kResultTrue, "setProcessing(true)");

  std::vector<float> inBuf((size_t) kBlock * nChans, 0.f);
  std::vector<float> outBuf((size_t) kBlock * nChans, 0.f);
  std::vector<float*> inPtrs((size_t) nChans), outPtrs((size_t) nChans);
  for (int32_t c = 0; c < nChans; c++)
  {
    inPtrs[c] = inBuf.data() + (size_t) c * kBlock;
    outPtrs[c] = outBuf.data() + (size_t) c * kBlock;
  }

  AudioBusBuffers inBuses {};
  inBuses.numChannels = nChans;
  inBuses.channelBuffers32 = inPtrs.data();

  AudioBusBuffers outBuses {};
  outBuses.numChannels = nChans;
  outBuses.channelBuffers32 = outPtrs.data();

  ProcessData data {};
  data.processMode = kRealtime;
  data.symbolicSampleSize = kSample32;
  data.numSamples = kBlock;
  data.numInputs = 1;
  data.numOutputs = 1;
  data.inputs = &inBuses;
  data.outputs = &outBuses;

  const int32_t nBlocks = (int32_t) std::lround(seconds * kRate / kBlock);
  std::vector<float> recorded;
  recorded.reserve((size_t) nBlocks * kBlock * nChans);

  for (int32_t b = 0; b < nBlocks; b++)
  {
    for (int32_t s = 0; s < kBlock; s++)
    {
      const double t = (double) (b * kBlock + s) / kRate;
      const float v = (float) (0.5 * std::sin(2.0 * M_PI * 220.0 * t));
      for (int32_t c = 0; c < nChans; c++)
        inPtrs[c][s] = v;
    }

    if (proc->process(data) != kResultTrue)
    {
      std::fprintf(stderr, "FAIL: process() returned false at block %d\n", b);
      return 1;
    }

    // Write interleaved stereo, like any normal .raw capture.
    for (int32_t s = 0; s < kBlock; s++)
      for (int32_t c = 0; c < nChans; c++)
        recorded.push_back(outPtrs[c][s]);
  }

  CHECK(proc->setProcessing(false) == kResultTrue, "setProcessing(false)");
  CHECK(comp->setActive(false) == kResultTrue, "setActive(false)");

  FILE* f = std::fopen(outPath, "wb");
  CHECK(f != nullptr, (std::string("fopen ") + outPath).c_str());
  const size_t written = std::fwrite(recorded.data(), sizeof(float), recorded.size(), f);
  std::fclose(f);
  CHECK(written == recorded.size(), "short write");

  // Report the level and where the energy sits, so a silent or wrong render
  // is obvious without a DAW.
  double peak = 0.0, sumSq = 0.0;
  for (float v : recorded)
  {
    const double a = std::fabs((double) v);
    if (a > peak) peak = a;
    sumSq += (double) v * (double) v;
  }
  const double rms = std::sqrt(sumSq / (double) recorded.size());

  std::printf("wrote %zu samples (%.3f s) to %s\n", recorded.size(), seconds, outPath);
  std::printf("peak %.6f  rms %.6f\n", peak, rms);
  CHECK(peak > 1e-4, "output is silent");
  CHECK(rms > 1e-5, "output rms is silent");

  std::printf("OK\n");
  return 0;
}
