---
id: evidence:meari/20261107174455
title: Native window rendering validation
status: active
created: 2026-11-07T17:44:55Z
updated: 2026-11-07T18:31:24Z
rel:
  governs: [crates/meari-native]
---

# Native window rendering validation

## Scope and reproduction

Baseline source: `1482b746ebc0d867f25d17d01c48c12df888ca6c`. Linux x86_64,
Rust 1.99.1; default Cargo release optimization (`opt-level=3`) versus the repository's
unoptimized dev profile. The CPU and software-GPU release runs used the same binary
built with `window-gpu`, synthetic 64-control document, theme, size sequence, and
window environment. No consuming application was changed or profiled.

```
cargo build --release -p meari-native-tools --features window-gpu --bin meari-resize-probe -j 1
# Repeat each three times, CPU/hardware-GPU attempts alternating:
meari-resize-probe cpu
meari-resize-probe gpu
meari-resize-probe gpu-software
cargo build -p meari-native-tools --features window-gpu --bin meari-resize-probe -j 1
# Repeat the dev binary's cpu probe three times.
```

Warmup requires an initial successful presentation and GPU completion, followed by
250ms. An independent producer then requests all 240 sizes at an 8ms sleep cadence.
A successful run must present the final requested size. All completed runs below
reported zero older-request-after-newer-presentation observations. Earlier startup
trials warmed from adapter selection and are excluded from the final measurements.

## End-to-end results

All values are milliseconds, nearest-rank quantiles. These are **present-call return
proxies, not actual-display latency**. Actual compositor display acknowledgement is
unobservable with this Linux wgpu/softbuffer path; every `resize_to_displayed` is `None`.
Queue completion is not a display acknowledgement.

### Producer resize request to present return

| Backend/profile | Run | Presented samples / 240 | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|---:|---:|
| Release CPU | 1 | 143 | 7.703 | 8.391 | 8.582 | 8.699 |
| Release CPU | 2 | 135 | 7.664 | 8.551 | 8.636 | 8.988 |
| Release CPU | 3 | 131 | 7.734 | 8.434 | 8.696 | 9.329 |
| Dev CPU | 1 | 1 | 185.777 | 185.777 | 185.777 | 185.777 |
| Dev CPU | 2 | 1 | 181.131 | 181.131 | 181.131 | 181.131 |
| Dev CPU | 3 | 1 | 178.863 | 178.863 | 178.863 | 178.863 |
| Release software GPU | 1 | 1 | 17.509 | 17.509 | 17.509 | 17.509 |
| Release software GPU | 2 | 1 | 18.892 | 18.892 | 18.892 | 18.892 |
| Release software GPU | 3 | 1 | 1353.805 | 1353.805 | 1353.805 | 1353.805 |
| Hardware GPU (required) | 1–3 | 0 | N/A | N/A | N/A | N/A |

**The one-sample rows are single observations, not usable p50/p95/p99 distributions.**
The repeated identical quantiles are mechanical output, not evidence about tails.

### Latest winit resize-event receipt to present return

This excludes OS/event dispatch backlog. The request clock above includes it.

| Backend/profile | Run | Samples | p50 | p95 | p99 | max | Maximum inter-present gap |
|---|---:|---:|---:|---:|---:|---:|---:|
| Release CPU | 1 | 143 | 4.517 | 5.298 | 5.517 | 5.520 | 47.686 |
| Release CPU | 2 | 135 | 4.499 | 5.319 | 5.525 | 5.537 | 127.906 |
| Release CPU | 3 | 131 | 4.579 | 5.405 | 5.768 | 6.125 | 73.081 |
| Dev CPU | 1 | 1 | 172.437 | 172.437 | 172.437 | 172.437 | 2127.266 |
| Dev CPU | 2 | 1 | 171.944 | 171.944 | 171.944 | 171.944 | 2124.066 |
| Dev CPU | 3 | 1 | 171.063 | 171.063 | 171.063 | 171.063 | 2123.158 |
| Release software GPU | 1 | 1 | 9.805 | 9.805 | 9.805 | 9.805 | 1955.312 |
| Release software GPU | 2 | 1 | 11.084 | 11.084 | 11.084 | 11.084 | 1959.873 |
| Release software GPU | 3 | 1 | 11.279 | 11.279 | 11.279 | 11.279 | 3293.911 |

### Run spread (do not pool quantiles)

- Release CPU: mean of per-run p50/single observations 7.700ms; range 7.664–7.734ms.
- Dev CPU: mean of per-run p50/single observations 181.924ms; range 178.863–185.777ms.
- Release software GPU: mean of per-run p50/single observations 463.402ms; range 17.509–1353.805ms.

## Backend and starvation interpretation

Hardware GPU selection failed explicitly on all three attempts:
`GPU hardware required; software adapter rejected: llvmpipe (LLVM 19.1.7, 256 bits)`.
The selected backend was `None`; these are not GPU successes and no hardware-GPU
performance comparison can be made on this machine.

The diagnostic software runs selected `VelloGpu`, `software_fallback=true`, adapter
`llvmpipe (LLVM 19.1.7, 256 bits) (Cpu, Gl)`. This is the wgpu surface renderer,
not the existing offscreen Skia GL/readback host. The CPU runs selected `VelloCpu`,
`software_fallback=false`.

Only the final resize was presented in the dev-CPU and software-GPU bursts. This is
not a stuck in-flight slot: all three runs of each finished all 240 requests and
presented request 239 without a completion timeout or stale-order observation.
The final dev frame spends about 165ms in CPU raster; the final software-GPU frame
costs about 10–11ms including encode/present. Both exceed the 8ms request cadence.
Strict post-raster latest-size rejection can discard repeated intermediate attempts
until resizing stops. The 2.1s dev and 2.0–3.3s software inter-present gaps expose this
starvation instead of hiding it behind a small final-frame latency.
The software run-3 1353.805ms request-to-present outlier is real; its 11.279ms
receipt-to-present clock does not explain the preceding dispatch delay. Discarded
attempts are not separately profiled, so its precise driver/dispatch cause remains
unverified. Do not recommend the software GPU as a latency improvement.

The optimized CPU is substantially cheaper per successful synthetic frame than dev,
but this is not an unchanged-code baseline experiment, nor proof of a hardware GPU
speedup or a fix for an application's reported 150–200ms delay. Coalesced requests
have no presentation sample; no zeros are invented for discarded requests. The
original application leak and host-side retained-source reprojection cost were not
diagnosed here.

## Staged timings

Each block contains the actual successful-frame stage quantiles (ms). GPU raster
means CPU encode/submit duration, **not device execution time**. CPU pixel copy is
RGBA conversion into softbuffer. End-to-end clocks include retries; stage clocks
cover only successful attempts. Do not sum quantiles from different stages.
No input contents, document text, raw AX text, or scene commands enter these metrics.

### Release CPU, run 1 (143 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.700,2.476,2.754,2.839
responsive_source_update,0.000,0.000,0.000,0.000
layout,0.314,1.212,1.530,1.644
scene_build,0.232,0.266,0.286,0.296
raster_encode_or_cpu_raster,1.542,1.843,1.884,1.912
pixel_copy,0.176,0.243,0.257,0.259
surface_acquire_configure,0.017,0.026,0.028,0.028
present_call,0.003,0.005,0.025,0.026
```

### Release CPU, run 2 (135 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.730,2.615,2.831,2.933
responsive_source_update,0.000,0.000,0.000,0.000
layout,0.322,1.176,1.278,1.510
scene_build,0.236,0.268,0.300,0.303
raster_encode_or_cpu_raster,1.567,1.803,1.873,1.968
pixel_copy,0.183,0.257,0.279,0.419
surface_acquire_configure,0.017,0.026,0.030,0.042
present_call,0.003,0.005,0.012,0.023
```

### Release CPU, run 3 (131 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.173,2.492,2.950,2.968
responsive_source_update,0.000,0.000,0.000,0.000
layout,0.348,1.338,1.648,2.026
scene_build,0.245,0.298,0.348,0.366
raster_encode_or_cpu_raster,1.626,1.962,2.093,2.108
pixel_copy,0.194,0.356,0.477,1.167
surface_acquire_configure,0.018,0.039,0.054,0.082
present_call,0.004,0.006,0.009,0.035
```

### Dev CPU, run 1 (1 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.154,0.154,0.154,0.154
responsive_source_update,0.000,0.000,0.000,0.000
layout,2.023,2.023,2.023,2.023
scene_build,1.100,1.100,1.100,1.100
raster_encode_or_cpu_raster,165.513,165.513,165.513,165.513
pixel_copy,3.043,3.043,3.043,3.043
surface_acquire_configure,0.122,0.122,0.122,0.122
present_call,0.042,0.042,0.042,0.042
```

### Dev CPU, run 2 (1 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.145,0.145,0.145,0.145
responsive_source_update,0.000,0.000,0.000,0.000
layout,1.978,1.978,1.978,1.978
scene_build,1.065,1.065,1.065,1.065
raster_encode_or_cpu_raster,164.516,164.516,164.516,164.516
pixel_copy,3.349,3.349,3.349,3.349
surface_acquire_configure,0.127,0.127,0.127,0.127
present_call,0.035,0.035,0.035,0.035
```

### Dev CPU, run 3 (1 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.434,0.434,0.434,0.434
responsive_source_update,0.000,0.000,0.000,0.000
layout,2.019,2.019,2.019,2.019
scene_build,1.063,1.063,1.063,1.063
raster_encode_or_cpu_raster,163.560,163.560,163.560,163.560
pixel_copy,3.174,3.174,3.174,3.174
surface_acquire_configure,0.084,0.084,0.084,0.084
present_call,0.034,0.034,0.034,0.034
```

### Release software GPU, run 1 (1 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.098,0.098,0.098,0.098
responsive_source_update,0.000,0.000,0.000,0.000
layout,0.118,0.118,0.118,0.118
scene_build,0.237,0.237,0.237,0.237
raster_encode_or_cpu_raster,6.468,6.468,6.468,6.468
pixel_copy,0.000,0.000,0.000,0.000
surface_acquire_configure,0.199,0.199,0.199,0.199
present_call,2.541,2.541,2.541,2.541
```

### Release software GPU, run 2 (1 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.429,0.429,0.429,0.429
responsive_source_update,0.000,0.000,0.000,0.000
layout,0.323,0.323,0.323,0.323
scene_build,0.262,0.262,0.262,0.262
raster_encode_or_cpu_raster,6.541,6.541,6.541,6.541
pixel_copy,0.000,0.000,0.000,0.000
surface_acquire_configure,0.200,0.200,0.200,0.200
present_call,2.553,2.553,2.553,2.553
```

### Release software GPU, run 3 (1 samples)

```csv
stage,p50_ms,p95_ms,p99_ms,max_ms
event_wait,0.164,0.164,0.164,0.164
responsive_source_update,0.000,0.000,0.000,0.000
layout,0.117,0.117,0.117,0.117
scene_build,0.314,0.314,0.314,0.314
raster_encode_or_cpu_raster,7.436,7.436,7.436,7.436
pixel_copy,0.000,0.000,0.000,0.000
surface_acquire_configure,0.293,0.293,0.293,0.293
present_call,2.821,2.821,2.821,2.821
```

## Correctness and build gates

Final completed command log: `meari-window-final-workspace-tests.log`:

- `cargo test --workspace --features meari-native/window-gpu -j 1 -- --test-threads=1`:
  **864 passed, 0 failed, 12 ignored**, across 121 result groups including doc tests.
- `cargo clippy --workspace --all-targets --features meari-native/window-gpu -j 1 -- -D warnings`:
  passed.
- `cargo test -p meari-native --features window --lib window::rendering -j 1 -- --test-threads=1`:
  **9 passed**, 199 filtered out. Includes no-feature GPU rejection, completion-bound
  lifetime/single-slot/timeout tests, stale-size rejection, unsupported-GPU-scene
  rejection, and quantile handling.
- `cargo fmt --all --check`: passed.
- `cargo tree -p meari-native --features window -e normal`: no wgpu, GPU Vello, or
  anyrender_vello dependency. CPU-only consumers do not opt into the GPU stack.
- Lock comparison: 43 packages added, zero existing package/version/source entries removed.
- Size-only unchanged-media projection regression failed before the fast-path fix and
  passed in the final suite; later media revision gaps, persistent input/focus/click
  state and AX node identity are covered. Strict unknown-theme behavior is tested;
  the fixture explicitly selects Light rather than inventing an OS theme.

Raw final probe logs are named `meari-window-final-{release-cpu,release-gpu,release-software,dev-cpu}-{1,2,3}.log`
in the local scratch output directory. They contain only content-free timings/status.

## Limits and release recommendation

Windows CEF child HWND attachment, focus, IME, AccessKit, DPI, maximize/restore/minimize
and shutdown integration were **not verified on this Linux box**. Existing ownership
and lifecycle paths remain in place; no sandbox flags or privileges were relaxed.
Wayland, hardware GPU throughput, real device-loss injection, and actual compositor
visibility need platform validation. Surface acquire/configure/present may still
throttle inside platform drivers even though the application does not synchronously
wait for GPU completion each frame.

One GPU frame is allowed in flight. Scene/texture leases remain in the queue callback;
Vello caches remain in the owner until completion. Size changes are merged into one
pending value; final geometry is checked after raster/encode. Shutdown drains caches
with a bounded off-UI-thread wait, cancelling the device on drain failure. Completion
timeout is an explicit error and never authorizes early reuse.

The opt-in GPU painter explicitly rejects filter/backdrop-filter and custom/resource
brushes rather than silently omitting them. Such documents should stay on CPU.

Recommend **0.3.0**, after reviewing these numbers and platform limitations: experimental
opt-in surface GPU rendering and backend/fallback diagnostics; latest-size resize
coalescing; persistent-DOM size-only layout refresh; staged latency metrics and native
probe; skip unchanged responsive-source projection. CPU remains the default.
No version bump, tag, release, or registry publication was performed by this change.
