#include "ResponseModel.h"
#include <cstdio>
// Prints, for each case, designBandT<double> magnitudes in dB: type f gain q slopeDb sr | values...
int main()
{
    const double freqs[] = { 20, 50, 100, 200, 300, 500, 700, 1000, 1500, 2000, 3000, 5000, 8000, 12000, 16000, 20000 };
    struct Case { float f, g, q; int slope; double sr; };
    std::vector<Case> cases;
    for (double sr : { 48000.0, 96000.0 })
        for (float g : { -9.0f, 6.0f })
            cases.push_back ({ g < 0 ? 120.0f : 1000.0f, g, g < 0 ? 0.7f : 1.0f, 12, sr });
    const int encoded[] = { ResponseModel::encodeSlopeNorm (0.0f), ResponseModel::encodeSlopeNorm (0.3f), ResponseModel::encodeSlopeNorm (0.5f), ResponseModel::encodeSlopeNorm (0.62f), ResponseModel::encodeSlopeNorm (0.9f), ResponseModel::encodeSlopeNorm (0.1f) };
    for (int e : encoded) { cases.push_back ({ 1000.0f, 6.0f, 1.0f, e, 48000.0 }); cases.push_back ({ 250.0f, -12.0f, 3.0f, e, 44100.0 }); }
    cases.push_back ({ 9000.0f, 12.0f, 0.4f, 24, 48000.0 });
    cases.push_back ({ 40.0f, -24.0f, 8.0f, 36, 48000.0 });
    for (int type = 0; type < filterTypeCount; ++type)
        for (const auto& c : cases)
        {
            auto d = ResponseModel::designBandT<double> ((FilterType) type, c.f, c.g, c.q, c.slope, c.sr);
            std::printf ("%d %.9g %.9g %.9g %d %.1f %d |", type, c.f, c.g, c.q, c.slope, c.sr, d.stageCount);
            for (double f : freqs)
            {
                double m = 1.0;
                for (int i = 0; i < d.stageCount; ++i)
                {
                    const auto& r = d.raw[(size_t) i];
                    std::array<float,6> rf;
                    ResponseModel::NormalizedStage st; st.order = 2;
                    const double inv = 1.0 / r[3];
                    double c5[5] = { r[0]*inv, r[1]*inv, r[2]*inv, r[4]*inv, r[5]*inv };
                    const double ff = std::min (f, c.sr * 0.5 * 0.999);
                    const auto z = std::polar (1.0, -2.0 * 3.141592653589793 * ff / c.sr);
                    const auto h = (c5[0] + c5[1] * z + c5[2] * z * z) / (1.0 + c5[3] * z + c5[4] * z * z);
                    m *= std::abs (h);
                    (void) rf; (void) st;
                }
                std::printf (" %.9g", 20.0 * std::log10 (std::max (m, 1e-15)));
            }
            std::printf ("\n");
        }
}
