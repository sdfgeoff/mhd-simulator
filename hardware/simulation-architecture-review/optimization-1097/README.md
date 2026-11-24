# Approved storage/work/submission optimizations

Implemented for topic25 reply1097: items1,2,4 from reply1065. The discretization, f32/high-low potential arithmetic, tolerances, convergence/current gates, pressure projections and preconditioners are unchanged. No new mechanical-design experiment or long flow run was launched.

## Changes

1. **Current-only basis reduction.** `solve_current` uses `collect_ports` for the unit and motional bases, retaining the original terminal-face arithmetic and reduction order. It does not reconstruct full J/force/heat fields for those two discarded intermediate outputs. The final combination still reconstructs physical force/heating and all reported power/current scalars. Conductivity is refreshed once before the two bases from the same accepted alpha; a standalone `solve()` still refreshes it each time.
2. **Compact coupled-flow packets.** `Conduction::new_compact_with_groups` packs the fixed B record entirely on GPU from64 to16 bytes/cell, and allocates a16-byte force.xyz/Joule-density output instead of64 bytes/cell. It does not retain the wide input through the unused flow binding. Full `CellField` constructors and diagnostic reads remain available. The compact constructor explicitly rejects full-field diagnostic reads rather than fabricating missing J/B records. Fluid forcing accepts either full or compact packets; the layout remains fixed for that fluid state, including frozen snapshots and updates.
3. **Bounded submission window.** Existing short command buffers retain their command counts, ordering, residual-check cadence and dispatch sizes. Instead of CPU-waiting after every bounded submit, the coupled-flow client waits after a window of4; CPU readbacks still wait for their data. The general library defaults to window1. `MHD_BOUNDED_SUBMISSION_WINDOW=1` restores that policy for the flow client; values1–8 are accepted. This reduces explicit host poll calls, not the number of GPU submissions or mathematical solver iterations. Pending-window bookkeeping is shared by cloned contexts.
4. Flow status JSON records the compact layouts, submission window and explicit bounded-submit/wait counts. Startup progress says “submitted” where a windowed stage has not yet been fenced.

## Memory and work accounting

1. Persistent allocation saving is **96N bytes**: two records each shrink64→16. At21,814,800 cell slots, that is **2,094,220,800 bytes =1.950 GiB**. The old approximately9.05GiB major-buffer estimate becomes approximately7.10GiB. This alone does **not** put that grid into6GiB.
2. At11,835,264 slots the same change saves approximately1.058GiB. No whole-device peak measurement was taken for a full assembly after this change. Startup still initially computes the general applied-EM packet/workspace before packing B; these persistent savings do not establish a startup peak or fit guarantee.
3. The two old intermediate output passes each wrote64N bytes; those wide stores are removed. Current-only reductions still launch over N, but nonterminal cells perform no field reconstruction. These are code/allocation facts, not measured DRAM-bandwidth savings.
4. No rollback buffer was removed and neither pressure projection was skipped. No preconditioner, active-cell compaction, physical resolution, waveform or tolerance change is included.

## Validation

All GPU checks below used the idle NVIDIA RTX3060 Laptop GPU through its explicit Vulkan ICD, serially. They are short regression cases, not full-size performance or AMD watchdog tests.

| Check | Result |
|---|---|
|`cargo test --offline --example conduction_probe -- --nocapture --test-threads=1`|11passed|
|`cargo test --offline --example conduction_flow -- --test-threads=1`|4passed (includes the same new electrical regression plus3 waveform tests)|
|`cargo test --offline --lib compact_force_matches_full_force -- --nocapture --test-threads=1`|1passed|
|`cargo test --offline --lib fluid::pressure_tests -- --nocapture --test-threads=1`|2passed|
|`cargo test --offline --lib impressed_force_work_is_separate -- --nocapture --test-threads=1`|1passed|
|`cargo check --offline --all-targets`|Passed; existing unused helper warning in `hardware_probe`|
|Scoped Clippy|Passed with the three existing lint categories below allowed|
|`git diff --check`|Passed|

The new electrical regression compares the original full-output algorithm, current-only output, and compact output with positive/zero/reverse current, changing partial liquid fractions, shear flow, nonuniform B, dissimilar conductivities and both contact controls. It forces multi-row dispatch on a small non-power-of-two grid. Terminal voltage, power/current scalars, full fields, compact force/heat and iteration counts match exactly. The existing suite also covers disconnected moving conductors, high conductivity contrast, predictor invalidation and contact-resistance laws.

The compact fluid regression compares shared full packets, shared compact packets and frozen compact snapshots through forcing, fluid integration, interface/momentum transport and the second projection. Final fluid and interface buffers match byte-for-byte; layout-changing updates are rejected. Pressure regression compares ordinary batching, bounded window1 and window4 for both existing Jacobi and multigrid paths; fields are byte-identical. Existing work-ledger checks pass.

In the small electrical comparison, explicit bounded-submit waits fall from139 to34 for one command and207 to51 for another. Required scalar readback waits are unchanged and are not included in that counter. Single-pass timing samples are retained in `validation.json`; they are not warmed, repeated performance benchmarks and establish no full-assembly speedup. Full-size AMD timeout behaviour under window4 has not been tested.

Strict Clippy initially encountered pre-existing `manual_is_multiple_of` (`src/run/export.rs`), `needless_range_loop` (`examples/conduction_probe.rs`) and `too_many_arguments` (`scheduled_current` in `examples/conduction_flow.rs`). No unrelated source was changed to silence those. The scoped check was:

```sh
cargo clippy --offline --lib --example conduction_probe --example conduction_flow -- \
  -D warnings -A clippy::manual_is_multiple_of \
  -A clippy::needless_range_loop -A clippy::too_many_arguments
```

[Machine-readable evidence](validation.json) includes test-output digests, sample counters and allocation arithmetic. Historical architecture/activity sheets describe the stopped pre-optimization V5; this note describes the new source path. Previously frozen experiment binaries/outputs remain preserved.
