# Performance

How close Linux gets to the hardware, measured on one tablet in October 2026, mostly against Android on the same
tablet. Single runs: expect a few percent between runs. Linux: Debian 13 with GNOME, Open Device Helper defaults.

## Summary

| Area | Android | Linux | Notes |
|---|---|---|---|
| CPU, Geekbench 7 single / multi | 3142 / 10520 | 3096 / 10121 (98 % / 96 %) | with memlat (patches 0120-0127); `kernel-t40` without it: 3038 / 9951 |
| Memory bandwidth (copy) | 25.7 GB/s | 25.6 GB/s | same |
| Memory latency, 64 MB random | 68 ns | 69 ns | `kernel-t40` without memlat: 127 ns |
| GPU compute, Geekbench 7 OpenCL | 19204 | 7559 (39 %) | Mesa 26.1.6 (Debian); about 14000 (73 %) with local Mesa changes, see [GPU](#gpu) |
| GPU raw throughput (clpeak) | — | FP32 3.05 TFLOPS, 66 GB/s | about 83 % of the theoretical 3.7 TFLOPS |
| Storage, sequential read / write | 2.7 / 1.8 GB/s | 4.3 / 2.0 GB/s | Android's /data is encrypted (inline crypto) |
| Storage, 4K random read QD1 / 4 jobs | 13.6k / 459k IOPS | 16.1k / 448k IOPS | with MCQ (patch 0119) |
| Video decode, 4K | — | H.264 319 fps, HEVC 531 fps | hardware decoder (iris); 40 Mbit/s test clips |

## Against published Android results

Where the same benchmark runs on both, Linux on this tablet against results published for other Snapdragon 8 Elite
Gen 5 devices (Android):

| Benchmark | Linux | Published (Android) |
|---|---|---|
| Geekbench 6 CPU single / multi | 3641 / 11175 | about 3655–3710 / 10758–11672 |
| Speedometer 3.1 (Chromium 150) | 22.5 | 18.1 (OnePlus 15), 23 (Xiaomi 17), 46 (Galaxy S26 Ultra) |
| JetStream 2.2 (Chromium 150) | 389.5 | about 281–295 (Chrome) |
| Geekbench 6 Vulkan | 18099 (Mesa 26.1.6), 20597 (Mesa main), about 23000–23300 (local Mesa changes) | about 27200–29700 |
| Geekbench 6 OpenCL | 9657 (Mesa main) | about 24000 |
| GravityMark 1.89 Vulkan, 1920×1080, 200k asteroids | 5482 (32.8 fps, Mesa 26.1.6) | no Adreno 840 entries; Adreno 830: 3530–4568 |

With Debian's Mesa 26.1.6, Geekbench 6 OpenCL fails one workload's result check (Particle Physics) and scores
2593. Graphics (GravityMark) lands where an Adreno 840 should be, above the Adreno 830 phones. So the loss is in GPU
compute (Geekbench's OpenCL and Vulkan compute workloads), not across the board.

## CPU and memory

Two kernel changes closed most of the CPU gap:

- **Patch 0117** gives the scheduler the two core sizes. Before it, all eight cores looked the same and a busy
  single thread could stay on a small core (Geekbench 6 single-core 2288 → 3590).
- **Patches 0120-0127** (Qualcomm's memlat series, pending upstream) start the memory-latency governor in the CPU
  control firmware. Without it, DDR stays at a low clock while the CPU waits on memory with little bandwidth in
  use, and random memory access takes twice as long as on Android.

| Kernel | GB7 single | GB7 multi |
|---|---|---|
| `kernel-t40` | 3038 | 9951 |
| `kernel-t40` with GNOME stopped | 3016 | 9999 |
| with memlat | 3096 | 10121 |
| Android | 3142 | 10520 |

Earlier Geekbench 6 runs on Linux: 2288 / 10359 (before 0117), 3590 / 11016 (with 0117), 3572 / 10857
(`kernel-t40`). Android results published for this tablet are about 3655–3710 / 10758–11672.

Per workload (Geekbench 7, memlat kernel against Android on the same tablet), single-core is uniformly about 97 %
of Android. Multi-core is level except three long all-core workloads: Asset Compression 83 %, Clang 89 %, Ray
Tracer 91 %. During them all eight cores are busy while the clocks fall within a second or two to 2.0–2.9 GHz
(small cores, 3.63 GHz maximum) and 2.7–3.1 GHz (large cores, 4.61 GHz) at about 100 °C; the operating system
reports no throttling, so the limit comes from the firmware. Whether Android sustains higher clocks there has not
been measured yet.

## GPU

The GPU itself runs as it should: clocks up to 1200 MHz (Android's limit too), FP32 3.05 TFLOPS and 66 GB/s
measured with clpeak. Graphics (OpenGL, Vulkan) go through Mesa's freedreno and Turnip drivers; Geekbench 6 Vulkan
gives **18342** (Android's 25622 is Geekbench 7, not comparable).

**OpenCL is the weak spot**: Geekbench 7 OpenCL is 39 % of Android with Debian's Mesa. The cause is Mesa's
OpenCL compiler (rusticl on freedreno), not the kernel; few desktop applications use OpenCL. What we found:

| Change | GB7 OpenCL |
|---|---|
| Mesa 26.1.6 (Debian 13 backports) | 7559 |
| Mesa main (26.3-devel) | 8632 |
| + buffers the CPU reads back allocated cached instead of write-combined | 9285 |
| + kernels recompiled for the work-group size of each launch | 10542 |
| + uniform loads broadcast, native sin/cos under fast relaxed math, fast-math fixes, no scalar ALU on the 840 | 13615 |
| + small private arrays kept in registers, small `__constant` lookup tables turned into selects | 13700–14200 |

These are changes to a local Mesa build (thirteen patches prepared for upstream), not in any release; with them
Geekbench 6 OpenCL goes from 9657 to 14768 and Geekbench 6 Vulkan from 20597 to about 23000–23300, and OpenCL-CTS shows no
regression in the suites run. What they fix: values read at the same address by every thread were loaded once per
thread; `sin`/`cos` went through a software implementation even under `-cl-fast-relaxed-math`; Mesa compiled
fast-relaxed-math kernels as exact (it ignored the SPIR-V `Fast` flag); on the Adreno 840 the scalar ALU made
uniform loops wait on every iteration; a private array initialized through a decayed pointer stayed in scratch
memory; an indexed read of a small `__constant` table became a memory load depending on another load.

The same kernels, run in a small test program on both systems (Qualcomm's driver on Android, the local Mesa on
Linux), now come out close: Mesa takes 1.17–1.25 times Qualcomm's time on the slowest of them (a particle loop,
tiled matrix multiply, convolution, edge thinning), is faster on others, and starts work faster (one launch and
wait: about 50 µs against 95 µs). The largest remaining gap is a colour lookup table (Geekbench's Video Filter, 41 %
of Android): neighbouring pixels read the same table entries, which Qualcomm's driver serves from the texture
cache (0.27 ms per 1920×1080 frame) while Mesa's OpenCL global loads bypass it (0.57 ms; 0.40 ms when the table
is read as an image in a test). Routing read-only buffer arguments through the texture path is a larger change in
Mesa and has not been done.

For Vulkan, Geekbench 6's shaders are the same code on Android and Linux (compared after capturing them on both
systems). Per workload against a Samsung phone with the same chip, Linux was weakest in Particle Physics (48 %) and
Gaussian Blur (57 %). The n-body loop of Particle Physics runs a number of iterations known only at run time, and
Mesa did not unroll such loops, so each iteration waited for its own memory read. A local Mesa change that unrolls
them by four (and lets the reads of the copies be issued together) raises Particle Physics from about 52000 to
60000 and the Vulkan score from about 22000 to 23000–23300.

## Storage

UFS 4.1, `fio` with `io_uring` on both, 8 GB file, direct I/O; Linux with patches 0119-0127.

| Test | Android | Linux |
|---|---|---|
| Sequential read, 1 MB, QD32 | 2686 MB/s | 4265 MB/s |
| Sequential write, 1 MB, QD32 | 1819 MB/s | 2001 MB/s |
| 4K random read, QD1 / QD32 / 4 jobs | 13.6k / 208k / 459k | 16.1k / 209k / 448k |
| 4K random write, QD1 / QD32 / 4 jobs | 25.9k / 50.8k / 48.2k | 28.6k / 38.5k / 49.1k |

Patch 0119 turns on the controller's multi-queue mode (MCQ): random reads with 4 jobs went from 240k to 448k IOPS.
The I/O engine matters at low queue depth: with `libaio` the same Linux system does 11.5k random reads at QD1.

## Running it yourself

```sh
./geekbench7 --cpu                                   # Geekbench 7 Linux AArch64 preview
sudo apt install mesa-opencl-icd clinfo clpeak
RUSTICL_ENABLE=freedreno ./geekbench7 --gpu OpenCL
RUSTICL_ENABLE=freedreno clpeak
sudo apt install tinymembench fio
```

The Geekbench preview builds need an Internet connection and upload every result to the Geekbench Browser.
Close other programs and keep the tablet on a charger with Bypass charging off.
