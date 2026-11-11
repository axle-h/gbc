# gbc

A Game Boy, Game Boy Color and Super Game Boy emulator, as a library.

- The full CPU, PPU, APU, timer, serial port, DMA and HDMA, and interrupts.
- DMG, CGB (VRAM and WRAM banking, palette RAM, double speed) and the CGB's compatibility mode,
  including the boot ROM's title-derived palettes.
- SGB packets, palettes, attributes and VRAM transfers, but not the border.
- MBC1, MBC2, MBC3 with the real-time clock, MBC5 and HuC1. A mapper it cannot emulate fails to load
  with a typed error.
- Band-limited audio resampled to any output rate.
- Save states, and battery-backed RAM you can dump and restore.

It passes blargg's `cpu_instrs`, `instr_timing` and `dmg_sound`, `dmg-acid2` and `cgb-acid2`, and
27 of mooneye's 28 MBC tests (all but MBC1 multicart). M-cycle memory timing and the DMG OAM
corruption bug are not modelled.

```rust,no_run
use gbc::cycles::MachineCycles;
use gbc::game_boy::GameBoy;
use gbc::joypad::JoypadButtonState;

let rom = std::fs::read("game.gb").unwrap();
let mut gb = GameBoy::try_dmg(&rom).expect("a cartridge gbc can run");
gb.core_mut().mmu_mut().audio_mut().set_output_sample_rate(48_000);

let mut samples = vec![0.0f32; 4096];
loop {
    gb.hold_buttons(JoypadButtonState { a: true, ..Default::default() });
    gb.run(MachineCycles::PER_FRAME);
    let pixels = gb.core().mmu().display(); // 160x144 colours
    let frames = gb.core_mut().mmu_mut().audio_mut().read_samples_f32(&mut samples); // stereo pairs
    # break;
}
```

## Licence

MIT OR Apache-2.0, at your option.
