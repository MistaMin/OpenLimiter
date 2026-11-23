// Reference runner for the comparison: drives the ORIGINAL DualSone header the way
// PluginProcessor::run does (setSettings once per block, applyInputGain, process with a trace).
//
// Build (from the crate folder):
//   c++ -std=c++17 -O2 -ffp-contract=off -I "$DUALSONE_DIR/Source" \
//       harness/harness.cpp -o target/harness_cpp
//
// Usage: harness_cpp script.txt in.f32 out.f32
//   script: "fs <rate>", "block <n>", "n <samples>", then "set <sample> <paramId> <plainValue>" lines,
//           each applied at the start of the first block whose start is >= sample (sample 0: before prepare).
//   in.f32:  left[n] then right[n], raw float32.
//   out.f32: left[n], right[n], slowGainDb[n], fastGainDb[n], slowEngineLoudness[n], raw float32.

#include "dsp/DualLoudnessCore.h"

#include <cstdio>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

namespace
{
// Defaults from Params.h.
std::map<std::string, float> defaults()
{
    return { { "detector", 0 }, { "mode", 0 }, { "threshold", -18 }, { "mix", 100 }, { "output", 0 },
             { "slowOn", 1 }, { "slowRatio", 2 }, { "slowAttack", 800 }, { "slowRelease", 2500 }, { "slowWindow", 1 },
             { "fastOn", 1 }, { "fastCrest", 6 }, { "fastRatio", 3 }, { "fastAttack", 10 }, { "fastRelease", 100 },
             { "fastLookahead", 100 }, { "input", 0 }, { "autoGain", 0 }, { "delta", 0 } };
}

// params::toSettings without JUCE (jlimit -> std::clamp).
dlc::Settings toSettings (std::map<std::string, float>& v)
{
    dlc::Settings s;
    s.kWeighted       = v["detector"] < 0.5f;
    s.mode            = (dlc::Mode) std::clamp ((int) std::lround (v["mode"]), 0, 3);
    s.mix             = v["mix"] * 0.01f;
    s.outputDb        = v["output"];
    s.inputDb         = v["input"];
    s.slowOn          = v["slowOn"] > 0.5f;
    s.thresholdDb     = v["threshold"];
    s.slowRatio       = v["slowRatio"];
    s.slowAttackMs    = v["slowAttack"];
    s.slowReleaseMs   = v["slowRelease"];
    const int window = std::clamp ((int) std::lround (v["slowWindow"]), 0, 2);
    s.slowWindowSeconds = window == 0 ? 0.2f : window == 2 ? 3.0f : 0.4f;
    s.fastOn          = v["fastOn"] > 0.5f;
    s.fastCrestDb     = v["fastCrest"];
    s.fastRatio       = v["fastRatio"];
    s.fastAttackMs    = v["fastAttack"];
    s.fastReleaseMs   = v["fastRelease"];
    s.lookahead       = v["fastLookahead"] * 0.01f;
    s.autoGain        = v["autoGain"] > 0.5f;
    s.delta           = v["delta"] > 0.5f;
    return s;
}

struct Change { long sample; std::string id; float value; };
} // namespace

int main (int argc, char** argv)
{
    if (argc != 4)
    {
        std::fprintf (stderr, "usage: harness_cpp script.txt in.f32 out.f32\n");
        return 2;
    }
    double fs = 48000.0;
    int block = 256;
    long n = 0;
    std::vector<Change> changes;
    {
        std::ifstream f (argv[1]);
        std::string line;
        while (std::getline (f, line))
        {
            std::istringstream ss (line);
            std::string key;
            ss >> key;
            if (key == "fs") ss >> fs;
            else if (key == "block") ss >> block;
            else if (key == "n") ss >> n;
            else if (key == "set") { Change c; ss >> c.sample >> c.id >> c.value; changes.push_back (c); }
        }
    }
    std::vector<float> l ((size_t) n), r ((size_t) n), gS ((size_t) n), gF ((size_t) n), lE ((size_t) n);
    {
        std::FILE* f = std::fopen (argv[2], "rb");
        if (! f || std::fread (l.data(), 4, (size_t) n, f) != (size_t) n || std::fread (r.data(), 4, (size_t) n, f) != (size_t) n)
            return 3;
        std::fclose (f);
    }

    auto values = defaults();
    size_t next = 0;
    auto applyUpTo = [&] (long start)
    {
        while (next < changes.size() && changes[next].sample <= start)
        {
            values[changes[next].id] = changes[next].value;
            ++next;
        }
    };

    dlc::DualLoudnessCore core;
    applyUpTo (0);
    core.setSettings (toSettings (values));
    core.prepare (fs, 2);

    for (long start = 0; start < n; start += block)
    {
        applyUpTo (start);
        core.setSettings (toSettings (values));
        const int len = (int) std::min ((long) block, n - start);
        float* ch[2] = { l.data() + start, r.data() + start };
        core.applyInputGain (ch, 2, len);
        dlc::Trace t {};
        t.slowGainDb = gS.data() + start;
        t.fastGainDb = gF.data() + start;
        t.slowEngineLoudness = lE.data() + start;
        core.process (ch, 2, len, &t);
    }

    std::FILE* f = std::fopen (argv[3], "wb");
    if (! f)
        return 4;
    for (auto* v : { &l, &r, &gS, &gF, &lE })
        std::fwrite (v->data(), 4, (size_t) n, f);
    std::fclose (f);
    std::printf ("latency %d\n", core.latencySamples());
    return 0;
}
