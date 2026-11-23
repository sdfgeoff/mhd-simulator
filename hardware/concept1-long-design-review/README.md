# One longer experiment per design

Human reply 992 supersedes the earlier early-screening and waveform-sweep workflow.

1. Let the existing NVIDIA 120 A run finish its planned3.6 ms. A0.6 ms image is a saved snapshot, not a stopping point. Some earlier cases were stopped at1.8–3.0 ms; do not infer that they could never release later.
2. The next prepared case uses the same full mechanical geometry, materials,120 A / 400 µs forward pulse and25 A / 133.33 µs reverse pulse, but85 µm cells,2 µs steps and a10 ms window. There are no further waveform variants queued. This is one finer long experiment, not a grid-convergence sweep.
3. Original heater and conducting surroundings remain present. The full CAD magnet sources are unchanged; isolated outer magnet occupancy is cropped as before. Flightspace extends toZ=-12.57mm. All22 bodies are represented and initial GPU voxelization has zero overlaps. This was geometry setup only, not another electrical/fluid experiment.
4. The isolated AMD RADV adapter is available using the existing local driver. It reports10.56 GiB device-local shared-memory budget and a2 GiB buffer limit. Estimated working use is9.52 GiB with a1.34 GiB largest field buffer. Actual native allocation must still pass its normal guards. Runtime on this integrated GPU may be long; no ETA is promised.
5. 85 µm gives about7 cells across the0.6 mm outlet versus6 previously: a modest spatial improvement, not qualified breakup.50 µm with the full flightspace would exceed the available budget. See preparation.json for estimates; estimates are not allocation tests.
6. Save accepted fields every0.5 ms. Disk admission requires all21 raw fields plus a5 GiB reserve. Completed fields are losslessly zstd-archived only after SHA verification; existing analysis transparently reads archives. The completed100 A baseline has already been archived with verified decoded hashes, freeing about4.3 GiB without losing field data.
7. `plan-draft.json` is PREPARED ONLY. Launch with the separate `scripts/run_design_experiment.py` only after the current NVIDIA controller and native process are gone and its result is reported. The new runner accepts exactly one job, verifies the isolated adapter, freezes inputs/binaries/driver, and refuses compute overlap. It does not automatically retry failures.
8. Keep the established1e-6 charge screening tolerance, physical equations and source quadrature. The120 billion source-work cap allows the larger grid's104.2 billion interactions; it changes an admission cap, not source accuracy. No precision campaign, zero-current control or additional short physics screens.
9. Initial and thermal assumptions remain inductionless, isothermal and ideal-contact. Report electrical energy error and any boundary contact; do not classify late boundary-interacting fields as free flight. Do not stop merely because an early snapshot remains attached.

## Actual startup result

The first85µm/10ms attempt failed before timesteps. The edge-source buffer requires2,170,783,616bytes, exceeding the adapter limit2,147,483,647bytes. The earlier largest-buffer estimate omitted this edge-based buffer. Total heap availability does not remove the per-buffer limit. Inputs, logs and failed state are preserved; no automatic retry or guard bypass was made. See `startup-failure-v1.json`. The preceding120A coarse run completed3.6ms with an attached column at every threshold, reported in topic25 reply999.

## Fitting domain correction V2

Removed10emptybottomplanes(0.85mm), retaining11.22mmflightspace. New265×240×343grid has an EM edge buffer2,109,404,096bytes below the2,147,483,647byte limit. GPUgeometrycheck found zerooverlaps and byteidenticalretainedmasks; all22bodycounts unchanged. MechanicalCAD, fullmagnet sources, waveform,85µm cells,2µs steps and10ms duration unchanged. V2 launched with freshplan/state/output and normalnativeguards; fullsolverstartup and firstacceptedstep still pending at20:32NZDT23Nov. No further jobs queued.

## Large-grid dispatch correction V3

V2 reached DCstartup but failed beforetimesteps:85,215workgroups exceeded the single-axis65,535limit. DCdispatch now uses multiple rows with matching flattened indices and padded-reduction guards. All10conduction tests pass onAMD, including forcedmultirow/nonmultiplelastgroup with exactfield/statistics equality. V3uses a separately named executable; oldfailedstates/binaries are preserved. SameV2project,hardware,pulse,resolution,duration,PDEandgates. See `dispatch-recovery-v3.json`; startupfit/firststep remains pending.

## Command submission correction V4

V3exported itsinitialfield, then theAMDkernel reported a gfxringtimeout/reset before anyacceptedstep. Suspectedcause:32CGiterations inoneunpreemptiblecommandbuffer onthelargegrid. V4fencesiteration-sized5commandchunks for multirowgrids, preservingcommandorder and32iterationresidualchecks. All10conductiontests pass; theforcedmultirowtest alsoexerciseschunking with exactfield/statistics agreement. Firststepcharge/fluidprogresslogsadded. Thisis aruntimehypothesis, notproof of theoffendingkernel. Sameproject/physics/waveform/numerics; failedV3retained. See `submission-recovery-v4.json`.

## Fluid submission correction V5

V4chargeaccepted in82.8s, thenAMDGPUtimeoutinfirstfluidstep. V5fenceslargegridfluidpressureandMGstages, preservingstageorderandconvergencecheckcadence. ForcedboundedvsoriginalGPUtest has exactpressure/velocityfields forJacobiandMG; analyticpressureL2<1e-4. Thisaddresses a suspectedlongsubmissionpath; exactoffendingkernel remainsunisolated. Firstfluidpredictor/viscosity/pressureprogresslogsadded. Samehardware,numerics,pulse,10msduration. See `pressure-submission-recovery-v5.json`; noacceptedfluidstepyet.
