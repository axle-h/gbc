# CLAUDE.md

@README.md

The README is imported above; do not repeat it here. The invariants and the traps are in two short
documents under `docs/`. **They are not loaded automatically.** Read the one for the area you are
about to touch, before touching it. Each rule names the comment in the code that carries the
argument, so the code stays the source of truth.

## Rules of the road

- **Always `--release`.** The tests emulate every frame.
- A test of the emulator runs on a test ROM under `gbc/src/roms/`, never on a commercial cartridge.
  `workload/` is our own, and a test breaks on its fixed addresses, so rebuild it with its `build.sh`.
- **`axle-h/poke-agent` is the heavy user**, and its pre-push gates are the real regression suite:
  after a change to timing, the APU or the save-state format, run them there against this checkout
  (`[patch.crates-io] gbc = { path = "../gbc/gbc" }` in its gitignored `.cargo/config.toml`) before
  releasing.
- `publish-gbc.yml` is trusted by crates.io by its file name, so renaming it means updating the
  crate's Trusted Publishing settings.

## Keeping it tidy

- **Let the code say it, and say the rest once.** A comment earns its place by carrying a constraint
  the code below obeys and nothing else pins, in one or two plain sentences. No history, no dates,
  no measurements, no "used to".
- **A new invariant goes first into a comment on the code it constrains, then as one line in the doc
  for its area pointing at it.** A rule a test already pins does not need a line at all.
- **Terse wins.**

| Doc | Read before |
|---|---|
| [emulator-core](docs/emulator-core.md) | `gbc/src/{mmu,mbc,ppu,savestate,schedule,cycles,game_boy}.rs`, adding or reordering a serialised field |
| [emulator-performance](docs/emulator-performance.md) | optimising `gbc/src/{ppu,core,opcode,mmu}.rs` or `gbc/src/audio/`, or building a rig to measure them |
