#pragma once
// Minimal stand-in for the framework helpers the Harmonic Compressor DSP headers use.
// Each helper is written out with the framework's tie and floor rules.
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <limits>
#include <vector>

namespace juce {
template <typename T> constexpr T jmax (T a, T b) { return a < b ? b : a; }
template <typename T> constexpr T jmin (T a, T b) { return b < a ? b : a; }
template <typename T> constexpr T jlimit (T lo, T hi, T v) { return v < lo ? lo : (hi < v ? hi : v); }
template <typename T> constexpr T jmap (T v, T a, T b) { return a + v * (b - a); }
template <typename T> struct MathConstants {
    static constexpr T pi = static_cast<T> (3.141592653589793238L);
    static constexpr T twoPi = static_cast<T> (2 * 3.141592653589793238L);
};
struct Decibels {
    template <typename T> static T decibelsToGain (T db, T minusInf = T (-100))
    { return db > minusInf ? std::pow (T (10.0), db * T (0.05)) : T(); }
    template <typename T> static T gainToDecibels (T g, T minusInf = T (-100))
    { return g > T() ? jmax (minusInf, static_cast<T> (std::log10 (g)) * T (20.0)) : minusInf; }
};
inline bool approximatelyEqual (float a, float b) {
    if (! (std::isfinite (a) && std::isfinite (b))) return a == b;
    const float d = std::abs (a - b);
    return d <= std::numeric_limits<float>::min() || d <= std::numeric_limits<float>::epsilon() * std::max (std::abs (a), std::abs (b));
}
namespace ValueSmoothingTypes { struct Linear {}; struct Multiplicative {}; }
template <typename F, typename S = ValueSmoothingTypes::Linear>
struct SmoothedValue {
    static constexpr bool mult = std::is_same<S, ValueSmoothingTypes::Multiplicative>::value;
    F current = mult ? F (1) : F (0), target = current, step = 0; int countdown = 0, stepsToTarget = 0;
    void reset (double sr, double secs) { stepsToTarget = (int) std::floor (secs * sr); setCurrentAndTargetValue (target); }
    void setCurrentAndTargetValue (F v) { target = current = v; countdown = 0; }
    bool isSmoothing() const { return countdown > 0; }
    F getCurrentValue() const { return current; }
    void setTargetValue (F v) {
        if (approximatelyEqual (v, target)) return;
        if (stepsToTarget <= 0) { setCurrentAndTargetValue (v); return; }
        target = v; countdown = stepsToTarget;
        if (mult) step = std::exp ((std::log (std::abs (target)) - std::log (std::abs (current))) / (F) countdown);
        else step = (target - current) / (F) countdown;
    }
    F getNextValue() {
        if (! isSmoothing()) return target;
        --countdown;
        if (isSmoothing()) { if (mult) current *= step; else current += step; }
        else current = target;
        return current;
    }
    F skip (int n) {
        if (n >= countdown) { setCurrentAndTargetValue (target); return target; }
        if (mult) current *= (F) std::pow (step, n); else current += step * (F) n;
        countdown -= n; return current;
    }
};
template <typename T> struct AudioBuffer {
    std::vector<std::vector<T>> d;
    AudioBuffer (int ch = 0, int n = 0) : d ((size_t) ch, std::vector<T> ((size_t) n)) {}
    int getNumChannels() const { return (int) d.size(); }
    int getNumSamples() const { return d.empty() ? 0 : (int) d[0].size(); }
    T* getWritePointer (int c) { return d[(size_t) c].data(); }
    const T* getReadPointer (int c) const { return d[(size_t) c].data(); }
};
}
