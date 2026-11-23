design_reference.cpp prints OpenEQ 1.0.1's ResponseModel::designBandT<double> magnitudes for all
48 types; tests/data/ref_designs.txt is its output. The shim header stands in for the framework
helpers the header uses (coefficient formulas written out). Build:
  clang++ -std=c++17 -O2 -ffp-contract=off -I tools/shim -I <OpenEQ>/Source tools/design_reference.cpp -o ref && ./ref > tests/data/ref_designs.txt
-ffp-contract=off matters: with fused multiply-add contraction the original's own root grouping
resolves exact ties differently (one high-shelf case moves by 0.012 dB). The Rust port matches
the non-contracted build.
