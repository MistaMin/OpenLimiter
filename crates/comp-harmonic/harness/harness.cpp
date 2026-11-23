// Reference harness: drives the original Harmonic Compressor DSP headers, unchanged, the way
// the plugin's processBlock does (at 1x oversampling, where no resampler is involved).
//
// Build (from the crate directory):
//   c++ -std=c++17 -O2 -ffp-contract=off -I harness/shim -I <HarmonicCompressor>/Source \
//       harness/harness.cpp -o target/harness_cpp
//
// Reads a job from stdin:
//   <cmd> <sampleRate> <blockSize> <frames> <keyed> <in.f32> <key.f32> <out.f32> <meter.f32> <nSets>
//   then nSets lines "<block> <paramIndex> <value>" (sorted by block; plain values as the Rust
//   module stores them, parameter order of comp-harmonic's idx).
// cmd = chain : the whole processor at 1x; in/key/out are interleaved stereo; meter gets
//               3 floats per block (deepest reduction dB, mode GR dB, Auto Gain applied dB).
//       alg   : the active mode alone; in is a mono detector stream, out the gain per sample.
//       harm  : the harmonic stage alone; in/out interleaved stereo.
#include "CompressorCore.h"
#include "DSP/TransientDesigner.h"
#include "DSP/LoudnessMatch.h"
#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

namespace {
enum P { MODE, THR, RATIO, ATT, REL, KNEE, MAKEUP, MIX, H2, H3, H4, H5, SC_EN, SC_LISTEN, SC_HP,
         AUTO_REL, AUTO_ATT, IN_GAIN, OUT_GAIN, OS_MODE, OS_FILTER, TR_EN, TR_ATT, TR_SUS, TR_SPEED,
         AUTO_GAIN, DELTA, KEY_SOURCE, COUNT };

std::vector<float> readFile (const std::string& path)
{
    std::vector<float> v;
    if (FILE* f = std::fopen (path.c_str(), "rb"))
    {
        std::fseek (f, 0, SEEK_END);
        const long n = std::ftell (f);
        std::fseek (f, 0, SEEK_SET);
        v.resize ((size_t) n / sizeof (float));
        if (std::fread (v.data(), sizeof (float), v.size(), f) != v.size()) std::exit (2);
        std::fclose (f);
    }
    return v;
}

void writeFile (const std::string& path, const std::vector<float>& v)
{
    FILE* f = std::fopen (path.c_str(), "wb");
    if (! f) std::exit (3);
    std::fwrite (v.data(), sizeof (float), v.size(), f);
    std::fclose (f);
}

struct Set { int block, index; float value; };

CompressorParams compParams (const float* v)
{
    CompressorParams p;
    p.thresholdDb = v[THR]; p.ratio = v[RATIO]; p.attackMs = v[ATT]; p.releaseMs = v[REL];
    p.kneeDb = v[KNEE]; p.makeupGainDb = v[MAKEUP]; p.mix = v[MIX] / 100.0f;
    p.autoRelease = v[AUTO_REL] > 0.5f; p.autoAttack = v[AUTO_ATT] > 0.5f;
    return p;
}

// The processor's state and processBlock, transcribed at factor 1 (the oversampler returns the
// main buffer itself and the dry alignment is a no-op).
struct Processor
{
    double hostSampleRate = 48000.0;
    CompressorEngine engine;
    hcdsp::TransientDesigner transientDesigner;
    hcdsp::LoudnessMatcher loudnessMatcher;
    juce::SmoothedValue<float, juce::ValueSmoothingTypes::Multiplicative> autoGainSmoothed, inputGainSmoothed, outputGainSmoothed;
    juce::SmoothedValue<float> deltaSmoothed, deltaReferenceSmoothed;
    juce::AudioBuffer<float> dryBuffer, referenceBuffer, loudnessInputBuffer;
    bool externalSidechainSeen = false, referenceRunning = false;
    float blockReduction = 0, lastGr = 0, autoGainAppliedDb = 0;

    void prepare (double sr, int block, const float* v)
    {
        hostSampleRate = sr;
        transientDesigner.prepare (sr);
        loudnessMatcher.prepare (sr);
        autoGainSmoothed.reset (sr, 0.100); autoGainSmoothed.setCurrentAndTargetValue (1.0f);
        deltaSmoothed.reset (sr, 0.020); deltaSmoothed.setCurrentAndTargetValue (v[DELTA] > 0.5f ? 1.0f : 0.0f);
        deltaReferenceSmoothed.reset (sr, 0.050); deltaReferenceSmoothed.setCurrentAndTargetValue (v[AUTO_GAIN] > 0.5f ? 1.0f : 0.0f);
        inputGainSmoothed.reset (sr, 0.010); inputGainSmoothed.setCurrentAndTargetValue (juce::Decibels::decibelsToGain (v[IN_GAIN]));
        outputGainSmoothed.reset (sr, 0.010); outputGainSmoothed.setCurrentAndTargetValue (juce::Decibels::decibelsToGain (v[OUT_GAIN]));
        juce::dsp::ProcessSpec spec; spec.sampleRate = sr; spec.maximumBlockSize = (unsigned) block; spec.numChannels = 2;
        engine.prepare (spec);
        engine.reset();
        dryBuffer = juce::AudioBuffer<float> (2, block);
        referenceBuffer = juce::AudioBuffer<float> (2, block);
        loudnessInputBuffer = juce::AudioBuffer<float> (2, block);
    }

    void process (juce::AudioBuffer<float>& mainBuffer, const juce::AudioBuffer<float>* extScBus, int numSamples, const float* v)
    {
        const int numChannels = 2;
        CompressorParams params = compParams (v);
        const auto mode = static_cast<CompressorMode> ((int) v[MODE]);
        const float scHpHz = v[SC_HP];
        const bool scEnabled = v[SC_EN] > 0.5f, scListen = v[SC_LISTEN] > 0.5f;
        const float inGainDb = v[IN_GAIN], outGainDb = v[OUT_GAIN];
        const bool trEnabled = v[TR_EN] > 0.5f;
        const bool autoGainOn = v[AUTO_GAIN] > 0.5f, deltaOn = v[DELTA] > 0.5f;
        const float h2 = v[H2] / 100.0f, h3 = v[H3] / 100.0f, h4 = v[H4] / 100.0f, h5 = v[H5] / 100.0f;

        for (int ch = 0; ch < numChannels; ++ch)
            for (int s = 0; s < numSamples; ++s)
            {
                referenceBuffer.getWritePointer (ch)[s] = mainBuffer.getReadPointer (ch)[s];
                loudnessInputBuffer.getWritePointer (ch)[s] = mainBuffer.getReadPointer (ch)[s];
            }

        inputGainSmoothed.setTargetValue (juce::Decibels::decibelsToGain (inGainDb));
        if (inputGainSmoothed.isSmoothing() || std::abs (inGainDb) > 1.0e-4f)
            for (int s = 0; s < numSamples; ++s)
            {
                const float g = inputGainSmoothed.getNextValue();
                for (int ch = 0; ch < numChannels; ++ch) mainBuffer.getWritePointer (ch)[s] *= g;
            }

        engine.ensureSampleRate (hostSampleRate);
        engine.updateBlockParameters (mode, params, scHpHz);
        auto* alg = engine.getActiveAlgorithm();
        auto& harmonics = engine.getHarmonics();
        auto& sidechain = engine.getSidechain();

        const bool scBusUsable = extScBus != nullptr && extScBus->getNumChannels() > 0 && extScBus->getNumSamples() >= numSamples;
        if (scBusUsable && ! externalSidechainSeen)
        {
            float mag = 0.0f;
            for (int ch = 0; ch < extScBus->getNumChannels(); ++ch)
                for (int s = 0; s < numSamples; ++s) mag = juce::jmax (mag, std::abs (extScBus->getReadPointer (ch)[s]));
            if (mag > 1.0e-7f) externalSidechainSeen = true;
        }
        const bool hasExternalSc = scBusUsable && externalSidechainSeen;

        const float makeupGainLinear = juce::Decibels::decibelsToGain (params.makeupGainDb);
        const float mixWet = params.mix;

        const bool referenceNeeded = ((deltaOn && ! scListen) || deltaSmoothed.getCurrentValue() > 0.0f)
                                  && (autoGainOn || deltaReferenceSmoothed.getCurrentValue() > 0.0f);
        referenceRunning = referenceNeeded;
        for (int ch = 0; ch < numChannels; ++ch)
            for (int s = 0; s < numSamples; ++s) dryBuffer.getWritePointer (ch)[s] = mainBuffer.getReadPointer (ch)[s];

        const int osFactor = 1;
        auto readRawSidechain = [&] (int ch, int osSampleIdx) -> float
        {
            if (! hasExternalSc) return mainBuffer.getReadPointer (ch)[osSampleIdx];
            const int scCh = juce::jmin (ch, extScBus->getNumChannels() - 1);
            const float* scPtr = extScBus->getReadPointer (scCh);
            const float basePos = static_cast<float> (osSampleIdx) / static_cast<float> (osFactor);
            const int i0 = juce::jlimit (0, numSamples - 1, static_cast<int> (basePos));
            const int i1 = juce::jmin (numSamples - 1, i0 + 1);
            const float frac = basePos - static_cast<float> (i0);
            return scPtr[i0] + frac * (scPtr[i1] - scPtr[i0]);
        };

        float minGain = 1.0f;
        for (int s = 0; s < numSamples; ++s)
        {
            float detector = 0.0f;
            float scCh[2] = { 0.0f, 0.0f };
            for (int ch = 0; ch < numChannels; ++ch)
            {
                const float rawSc = readRawSidechain (ch, s);
                const float scVal = scEnabled ? sidechain.processSample (ch, rawSc) : rawSc;
                scCh[ch] = scVal;
                detector = juce::jmax (detector, std::abs (scVal));
            }
            const float gr = alg->processSidechain (detector);
            minGain = juce::jmin (minGain, gr);
            for (int ch = 0; ch < numChannels; ++ch)
            {
                auto* data = mainBuffer.getWritePointer (ch);
                if (scListen) data[s] = scCh[ch];
                else data[s] = harmonics.processSample (data[s] * gr * makeupGainLinear, h2, h3, h4, h5, ch);
            }
        }
        blockReduction = -juce::Decibels::gainToDecibels (minGain, -120.0f);

        if (! scListen && mixWet < 1.0f)
            for (int ch = 0; ch < numChannels; ++ch)
            {
                auto* c = mainBuffer.getWritePointer (ch);
                const float* d = dryBuffer.getReadPointer (ch);
                for (int s = 0; s < numSamples; ++s) c[s] = mixWet * c[s] + (1.0f - mixWet) * d[s];
            }

        if (! scListen)
            transientDesigner.processBlock (mainBuffer, numSamples, trEnabled, v[TR_ATT], v[TR_SUS], v[TR_SPEED]);
        if (! scListen)
            loudnessMatcher.process (loudnessInputBuffer, mainBuffer, numSamples);

        const float autoGainTargetDb = (autoGainOn && ! scListen) ? loudnessMatcher.getTargetGainDb() : 0.0f;
        autoGainSmoothed.setTargetValue (juce::Decibels::decibelsToGain (autoGainTargetDb));
        if (autoGainSmoothed.isSmoothing() || std::abs (autoGainSmoothed.getCurrentValue() - 1.0f) > 1.0e-6f)
            for (int s = 0; s < numSamples; ++s)
            {
                const float g = autoGainSmoothed.getNextValue();
                for (int ch = 0; ch < numChannels; ++ch) mainBuffer.getWritePointer (ch)[s] *= g;
            }
        autoGainAppliedDb = juce::Decibels::gainToDecibels (autoGainSmoothed.getCurrentValue(), -100.0f);

        deltaSmoothed.setTargetValue ((deltaOn && ! scListen) ? 1.0f : 0.0f);
        deltaReferenceSmoothed.setTargetValue (autoGainOn ? 1.0f : 0.0f);
        if (deltaSmoothed.isSmoothing() || deltaSmoothed.getCurrentValue() > 0.0f)
        {
            for (int s = 0; s < numSamples; ++s)
            {
                const float amount = deltaSmoothed.getNextValue();
                const float useInput = deltaReferenceSmoothed.getNextValue();
                for (int ch = 0; ch < numChannels; ++ch)
                {
                    const float dry = dryBuffer.getReadPointer (ch)[s];
                    const float reference = dry + useInput * (referenceBuffer.getReadPointer (ch)[s] - dry);
                    mainBuffer.getWritePointer (ch)[s] -= amount * reference;
                }
            }
        }
        else
            deltaReferenceSmoothed.skip (numSamples);

        outputGainSmoothed.setTargetValue (juce::Decibels::decibelsToGain (outGainDb));
        if (outputGainSmoothed.isSmoothing() || std::abs (outGainDb) > 1.0e-4f)
            for (int s = 0; s < numSamples; ++s)
            {
                const float g = outputGainSmoothed.getNextValue();
                for (int ch = 0; ch < numChannels; ++ch) mainBuffer.getWritePointer (ch)[s] *= g;
            }
        lastGr = alg->getLastGainReductionDb();
    }
};
}

int main()
{
    std::string cmd, inPath, keyPath, outPath, meterPath;
    double sr; int block, frames, keyed, nSets;
    std::cin >> cmd >> sr >> block >> frames >> keyed >> inPath >> keyPath >> outPath >> meterPath >> nSets;
    std::vector<Set> sets ((size_t) nSets);
    for (auto& s : sets) std::cin >> s.block >> s.index >> s.value;

    float v[COUNT] = {};
    size_t next = 0;
    auto applySets = [&] (int b) { while (next < sets.size() && sets[next].block <= b) { v[sets[next].index] = sets[next].value; ++next; } };
    applySets (0);

    const auto in = readFile (inPath);
    std::vector<float> out, meter;
    const int nBlocks = (frames + block - 1) / block;

    if (cmd == "chain")
    {
        const auto key = keyed ? readFile (keyPath) : std::vector<float>();
        Processor proc;
        proc.prepare (sr, block, v);
        out.resize ((size_t) frames * 2);
        for (int b = 0; b < nBlocks; ++b)
        {
            applySets (b);
            const int start = b * block, n = std::min (block, frames - start);
            juce::AudioBuffer<float> buf (2, n), kb (2, n);
            for (int s = 0; s < n; ++s)
                for (int ch = 0; ch < 2; ++ch)
                {
                    buf.getWritePointer (ch)[s] = in[(size_t) (start + s) * 2 + (size_t) ch];
                    if (keyed) kb.getWritePointer (ch)[s] = key[(size_t) (start + s) * 2 + (size_t) ch];
                }
            const bool external = keyed && v[KEY_SOURCE] > 0.5f;
            proc.process (buf, external ? &kb : nullptr, n, v);
            for (int s = 0; s < n; ++s)
                for (int ch = 0; ch < 2; ++ch) out[(size_t) (start + s) * 2 + (size_t) ch] = buf.getReadPointer (ch)[s];
            meter.push_back (proc.blockReduction);
            meter.push_back (proc.lastGr);
            meter.push_back (proc.autoGainAppliedDb);
        }
    }
    else if (cmd == "alg")
    {
        CompressorEngine engine;
        juce::dsp::ProcessSpec spec; spec.sampleRate = sr;
        engine.prepare (spec);
        engine.reset();
        out.resize ((size_t) frames);
        for (int b = 0; b < nBlocks; ++b)
        {
            applySets (b);
            engine.updateBlockParameters (static_cast<CompressorMode> ((int) v[MODE]), compParams (v), v[SC_HP]);
            auto* alg = engine.getActiveAlgorithm();
            const int start = b * block, n = std::min (block, frames - start);
            for (int s = 0; s < n; ++s) out[(size_t) (start + s)] = alg->processSidechain (in[(size_t) (start + s)]);
            meter.push_back (alg->getLastGainReductionDb());
        }
    }
    else if (cmd == "harm")
    {
        HarmonicsEngine h;
        juce::dsp::ProcessSpec spec; spec.sampleRate = sr;
        h.prepare (spec);
        out.resize ((size_t) frames * 2);
        for (int b = 0; b < nBlocks; ++b)
        {
            applySets (b);
            const int start = b * block, n = std::min (block, frames - start);
            for (int s = 0; s < n; ++s)
                for (int ch = 0; ch < 2; ++ch)
                {
                    const size_t i = (size_t) (start + s) * 2 + (size_t) ch;
                    out[i] = h.processSample (in[i], v[H2] / 100.0f, v[H3] / 100.0f, v[H4] / 100.0f, v[H5] / 100.0f, ch);
                }
        }
    }
    else
        return 1;

    writeFile (outPath, out);
    writeFile (meterPath, meter);
    return 0;
}
