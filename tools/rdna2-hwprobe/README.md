# rdna2-hwprobe

Runs hand-written RDNA2 kernels on a real AMD GPU and records exactly what the hardware returns.
It settles questions the public ISA documentation leaves open. Currently it probes
`tbuffer_{load,store}_format_d16_*` (and 32-bit partial stores) for every unified buffer format.
The translator's MTBUF d16 lowering (`crates/tombox-xshader/src/hlsl/mtbuf.rs`) and its test
model (`tests/exec_mem/mtbuf.rs`, hardware table `tests/exec_mem/mtbuf_hw.rs`) follow these results.

No game or console files are involved: the kernels are generated from scratch and the inputs are
synthetic.

## Requirements

- Linux with an RDNA2 GPU on the amdgpu driver (`/dev/kfd`). Recorded on the gfx1036 iGPU of a
  Ryzen 5 9600X; any RDNA2 part should do with `GFX=gfx10xx`.
- ROCm HIP runtime and headers, plus LLVM 18. On Ubuntu 24.04 / Mint 22:
  `sudo apt install rocminfo libamdhip64-dev clang-18 lld-18`.
  `rocminfo` should list the GPU agent (e.g. `gfx1036`).

## Run

```
tools/rdna2-hwprobe/run.sh /tmp/probe      # builds runner + kernels, runs, checks both models
python3 tools/rdna2-hwprobe/analyze_loads.py /tmp/probe   # candidate narrowing rules, layout
python3 tools/rdna2-hwprobe/nan_rule.py /tmp/probe        # fp32 -> fp16 NaN rule
python3 tools/rdna2-hwprobe/vectors.py /tmp/probe         # regenerate tests/exec_mem/mtbuf_hw.rs body
```

Each format gets one load kernel and one store kernel, run on 65536 threads.

- **Inputs:** every 8- and 16-bit component value appears in every component position. 32-bit
  components get edge values (rounding ties, the fp16 overflow boundary, subnormals, NaNs, int
  extremes) and then random values.
- **Loads:** registers are preset to sentinels `0xdeadbeef` / `0xcafef00d`, so preserved halves
  are visible.
- **Stores:** the output starts filled with `0xAA`, so partial writes are visible.

`partial_store32.py gen <dir>` then build/run, followed by `partial_store32.py show <dir>`,
does the same check for the 32-bit store forms.

## Results (gfx1036)

These results come from 77 formats x 65536 elements. `model.py` and `analyze_stores.py` report 0
mismatches. The Rust model in the tests matched all 5.0M load and 14.4M store elements.

**d16 loads**

- Two components per VGPR: c0/c1 in `vdata` lo/hi, c2/c3 in `vdata+1`.
- `_x` and `_xyz` leave the upper half of their last VGPR unchanged.
- Components the format lacks read 0, w included. The usual w = 1 comes from the descriptor's
  dst_sel, and these kernels use an identity swizzle.
- unorm / snorm / uscaled / sscaled: the exact value is rounded once to the nearest-even fp16,
  saturating at +-65504. It is not rounded through fp32: unorm16 65519 gives `0x3bff`, where
  fp32 would give `0x3c00`.
- 32-bit float: rounded toward zero. Finite overflow gives +-65504 and infinity stays infinity.
- NaN: `sign | 0x7c00 | ((mant >> 13) or 1)`. The payload is truncated and not quieted.
- 32-bit int: clamped to 16 bits, never truncated.
- 11-/10-bit float: exact expansion.

**d16 stores**

- The fp16 input is widened exactly; a NaN keeps its payload.
- unorm: `round_ne(saturate(x) * max)`.
- snorm: halves round away from zero (2-bit field: +-0.5 gives +-1).
- uint / sint: the 16-bit input is zero- or sign-extended, then clamped.
- uscaled / sscaled / 11-/10-bit float stores follow none of the simple rules tried; they are
  not lowered.

**Partial stores (d16 and 32-bit):** storing fewer components than the format has writes the whole
element, with the missing components as 0. For example, `tbuffer_store_format_x` of 0.25 to
`8_8_8_8_UNORM` writes `40 00 00 00`.
