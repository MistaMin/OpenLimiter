suppressor_reference.cpp runs OpenEQ 1.0.1's ResonanceEngine.h (through the shim header, which
supplies a plain FFT and the few helpers it uses). examples/compare_cpp.rs feeds the same signal
to both and prints the largest difference (3.7e-8 at 48 kHz, depth 0.8).
  clang++ -std=c++17 -O2 -ffp-contract=off -I tools/shim -I <OpenEQ>/Source tools/suppressor_reference.cpp -o reso
  RESO_CPP=./reso cargo run --release --example compare_cpp
