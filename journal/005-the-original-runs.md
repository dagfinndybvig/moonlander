# 005 The original runs

Date: 2026-10-14

## What was built

Two crates, written from DEC's manuals rather than from another emulator's code:

- `crates/gt40`: the VT11 display processor (display file execution with DPU timing, vector normalization, the 400 ns character clock, mid-vector light pen stops and resume, 12-bit position counters clipped to the 1024 x 768 face, dash patterns, blink, sync to the mains), the character ROM, the absolute loader format, and a plain rasterizer for snapshots.
- `crates/kd11b`: the PDP-11/05 processor with the basic instruction set only (anything the KD11-B lacks takes the reserved instruction trap), MM11 core, KW11-L line clock, DL11 host line, LK40 keyboard registers, interrupt priorities, WAIT and RESET, on one time base shared with the VT11.

The only code taken from elsewhere is the character ROM data, extracted verbatim from open-simh `display/vt11.c` under its MIT licence, with the notice kept. Condition code rules for NEG, ADC and SBC were checked against simh's PDP-11 as an independent reference.

## Result

The Mare Assabet binary, loaded unmodified into an emulated 8K-word GT40 at 60 Hz:

1. runs its start-up, probes the serial line, and sends `NUL ^C ^C K/F CR` to the host;
2. shows the greeting for 30 seconds;
3. starts the game. At 31 s the lander is at the top left of the lunar overview at -70 degrees with its engine burning, the readouts show height, distance and both velocities, the throttle bar stands at 75%, and the readout menu runs down the right. Five seconds later the lander has moved right and down and every readout has changed consistently with the physics.

`docs/images/emulated-gt40-*.png` are those frames, drawn by the plain rasterizer (no phosphor simulation).

## Pacing

During flight the program computes with `CLOCK` = 5: five 60 Hz ticks pass per physics step, about 12 steps a second. The 1973 listing's extra loop "WAIT FOR CLOCK ON THE 40 AND 45" forces a minimum of 5 ticks on faster processors, which suggests 5 was roughly the GT40's natural pace. The instruction timings used here are provisional (a first-order model in `crates/kd11b/src/timing.rs`), so the agreement is suggestive only, until DEC's KD11-B tables replace the model.

## Not yet verified

- Instruction semantics have been exercised by the program running correctly, not yet compared instruction by instruction against an independent emulator.
- Light pen interaction with the running program.
- The 1973 listing's build, which needs more memory than a stock GT40.
