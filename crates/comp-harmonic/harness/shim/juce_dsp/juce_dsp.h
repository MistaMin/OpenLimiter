#pragma once
#include "../juce_audio_basics/juce_audio_basics.h"
namespace juce { namespace dsp { struct ProcessSpec { double sampleRate = 44100.0; unsigned maximumBlockSize = 512; unsigned numChannels = 2; }; } }
