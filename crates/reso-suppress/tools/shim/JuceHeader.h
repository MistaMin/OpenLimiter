#pragma once
// Minimal stand-in for the few framework helpers ResponseModel.h / FilterDesign.h use.
#include <cmath>
#include <array>
#include <algorithm>
#include <complex>
#include <vector>
#include <memory>
#include <atomic>
namespace juce {
template <typename T> T jmin (T a, T b) { return a < b ? a : b; }
template <typename T> T jmax (T a, T b) { return a > b ? a : b; }
template <typename T> T jmin (T a, T b, T c) { return jmin (a, jmin (b, c)); }
template <typename T> T jlimit (T lo, T hi, T v) { return v < lo ? lo : (hi < v ? hi : v); }
inline int roundToInt (float v) { return (int) std::lround (v); }
template <typename T> struct MathConstants { static constexpr T pi = (T) 3.141592653589793238; static constexpr T twoPi = (T) 6.283185307179586477; };
struct Decibels { template <typename T> static T decibelsToGain (T db, T minus = (T) -100) { return db > minus ? std::pow ((T) 10, db * (T) 0.05) : (T) 0; } };
using int64 = long long; using uint32 = unsigned int;
namespace dsp {
template <typename T> using Complex = std::complex<T>;
struct FFT {
  int n; explicit FFT (int order) : n (1 << order) {}
  void perform (const std::complex<float>* in, std::complex<float>* out, bool inverse) const {
    std::vector<std::complex<double>> a (in, in + n);
    int bits = 0; while ((1 << bits) < n) ++bits;
    for (int i = 0; i < n; ++i) { int j = 0; for (int b = 0; b < bits; ++b) if (i & (1 << b)) j |= 1 << (bits - 1 - b); if (j > i) std::swap (a[i], a[j]); }
    for (int len = 2; len <= n; len <<= 1) { double ang = (inverse ? 2 : -2) * 3.141592653589793 / len;
      for (int s = 0; s < n; s += len) for (int k = 0; k < len / 2; ++k) { auto w = std::polar (1.0, ang * k); auto u = a[s + k], v = a[s + k + len / 2] * w; a[s + k] = u + v; a[s + k + len / 2] = u - v; } }
    for (int i = 0; i < n; ++i) out[i] = std::complex<float> ((float) (a[i].real() / (inverse ? n : 1)), (float) (a[i].imag() / (inverse ? n : 1)));
  }
};
namespace IIR {
template <typename N> struct ArrayCoefficients {
  static std::array<N,4> makeFirstOrderLowPass (double sr, N f) { auto n = std::tan (MathConstants<N>::pi * f / (N) sr); return { n, n, n + 1, n - 1 }; }
  static std::array<N,4> makeFirstOrderHighPass (double sr, N f) { auto n = std::tan (MathConstants<N>::pi * f / (N) sr); return { 1, -1, n + 1, n - 1 }; }
  static std::array<N,6> makeLowPass (double sr, N f, N Q) { auto n = 1 / std::tan (MathConstants<N>::pi * f / (N) sr); auto n2 = n * n; auto iq = 1 / Q; auto c1 = 1 / (1 + iq * n + n2); return { c1, c1 * 2, c1, 1, c1 * 2 * (1 - n2), c1 * (1 - iq * n + n2) }; }
  static std::array<N,6> makeHighPass (double sr, N f, N Q) { auto n = std::tan (MathConstants<N>::pi * f / (N) sr); auto n2 = n * n; auto iq = 1 / Q; auto c1 = 1 / (1 + iq * n + n2); return { c1, c1 * -2, c1, 1, c1 * 2 * (n2 - 1), c1 * (1 - iq * n + n2) }; }
  static std::array<N,6> makeBandPass (double sr, N f, N Q) { auto n = 1 / std::tan (MathConstants<N>::pi * f / (N) sr); auto n2 = n * n; auto iq = 1 / Q; auto c1 = 1 / (1 + iq * n + n2); return { c1 * n * iq, 0, -c1 * n * iq, 1, c1 * 2 * (1 - n2), c1 * (1 - iq * n + n2) }; }
  static std::array<N,6> makeNotch (double sr, N f, N Q) { auto n = 1 / std::tan (MathConstants<N>::pi * f / (N) sr); auto n2 = n * n; auto iq = 1 / Q; auto c1 = 1 / (1 + n * iq + n2); auto b0 = c1 * (1 + n2); auto b1 = 2 * c1 * (1 - n2); return { b0, b1, b0, 1, b1, c1 * (1 - n * iq + n2) }; }
  static std::array<N,6> makeAllPass (double sr, N f, N Q) { auto n = 1 / std::tan (MathConstants<N>::pi * f / (N) sr); auto n2 = n * n; auto iq = 1 / Q; auto c1 = 1 / (1 + iq * n + n2); auto b0 = c1 * (1 - n * iq + n2); auto b1 = c1 * 2 * (1 - n2); return { b0, b1, 1, 1, b1, b0 }; }
  static std::array<N,6> makeLowShelf (double sr, N f, N Q, N g) { auto A = jmax ((N) 0, std::sqrt (g)); auto am = A - 1, ap = A + 1; auto w = (2 * MathConstants<N>::pi * jmax (f, (N) 2)) / (N) sr; auto c = std::cos (w); auto beta = std::sin (w) * std::sqrt (A) / Q; auto amc = am * c;
    return { A * (ap - amc + beta), A * 2 * (am - ap * c), A * (ap - amc - beta), ap + amc + beta, -2 * (am + ap * c), ap + amc - beta }; }
  static std::array<N,6> makeHighShelf (double sr, N f, N Q, N g) { auto A = jmax ((N) 0, std::sqrt (g)); auto am = A - 1, ap = A + 1; auto w = (2 * MathConstants<N>::pi * jmax (f, (N) 2)) / (N) sr; auto c = std::cos (w); auto beta = std::sin (w) * std::sqrt (A) / Q; auto amc = am * c;
    return { A * (ap + amc + beta), A * -2 * (am + ap * c), A * (ap + amc - beta), ap - amc + beta, 2 * (am - ap * c), ap - amc - beta }; }
  static std::array<N,6> makePeakFilter (double sr, N f, N Q, N g) { auto A = jmax ((N) 0, std::sqrt (g)); auto w = (2 * MathConstants<N>::pi * jmax (f, (N) 2)) / (N) sr; auto alpha = std::sin (w) / (Q * 2); auto c2 = -2 * std::cos (w); auto at = alpha * A, ao = alpha / A; return { 1 + at, c2, 1 - at, 1 + ao, c2, 1 - ao }; }
};
}}
}
