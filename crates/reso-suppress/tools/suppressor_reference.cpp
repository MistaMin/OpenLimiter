#include "ResonanceEngine.h"
#include <cstdio>
#include <cstdint>
// Reads float32 stereo-identical input from stdin (n samples), writes processed left channel as float32.
int main (int argc, char** argv)
{
    const int n = atoi (argv[1]); const float depth = (float) atof (argv[2]);
    std::vector<float> x (n); fread (x.data(), 4, n, stdin);
    ResonanceEngine e; e.prepare (48000.0);
    ResonanceEngine::Settings s; s.depth = depth;
    auto r = x;
    for (int p = 0; p < n; p += 512) e.process (x.data() + p, r.data() + p, std::min (512, n - p), s);
    fwrite (x.data(), 4, n, stdout);
}
