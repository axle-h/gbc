//! Cartridges built in the test, for behaviour that depends on the header rather than the program.

/// A cartridge licensed to Nintendo and asking for SGB functions, with `title` and the given type
/// and size bytes. Its `2 << rom_size` banks are each filled with their own number, and the entry
/// point spins at `0x0150`.
pub fn cartridge(title: &str, cart_type: u8, rom_size: u8, ram_size: u8) -> Vec<u8> {
    const BANK: usize = 0x4000;
    let mut rom: Vec<u8> = (0..BANK << (rom_size + 1)).map(|i| (i / BANK) as u8).collect();
    rom[0x100..0x104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]); // nop; jp $0150
    rom[0x150..0x152].copy_from_slice(&[0x18, 0xFE]); // jr @
    rom[0x134..0x144].fill(0);
    rom[0x134..0x134 + title.len()].copy_from_slice(title.as_bytes());
    rom[0x144..0x146].copy_from_slice(b"01");
    rom[0x146] = 0x03;
    rom[0x147] = cart_type;
    rom[0x148] = rom_size;
    rom[0x149] = ram_size;
    rom[0x14A] = 0x01;
    rom[0x14B] = 0x33;
    rom[0x14C] = 0x00;
    fix_header_checksum(&mut rom);
    rom
}

/// Pokémon Red's header: a title the CGB boot ROM's palette table knows, on MBC3 with RAM and a
/// battery, 64 ROM banks and four of RAM.
pub fn pokemon_red_header() -> Vec<u8> {
    cartridge("POKEMON RED", 0x13, 0x05, 0x03)
}

/// The workload ROM under another title, which is all the CGB boot ROM chooses a palette by.
pub fn retitled_workload(title: &str) -> Vec<u8> {
    let mut rom = crate::roms::workload::ROM.to_vec();
    rom[0x134..0x143].fill(0);
    rom[0x134..0x134 + title.len()].copy_from_slice(title.as_bytes());
    fix_header_checksum(&mut rom);
    rom
}

fn fix_header_checksum(rom: &mut [u8]) {
    rom[0x14D] = rom[0x134..0x14D].iter().fold(0u8, |sum, &b| sum.wrapping_sub(b).wrapping_sub(1));
}
