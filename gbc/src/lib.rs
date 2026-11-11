#![doc = include_str!("../README.md")]

mod activation;
pub mod audio;
pub mod boot_palette;
mod cgb_palette;
pub mod core;
pub mod cycles;
pub mod divider;
pub mod game_boy;
pub mod geometry;
mod hdma;
pub mod header;
mod interrupt;
pub mod joypad;
pub mod lcd_control;
mod lcd_dma;
pub mod lcd_palette;
mod lcd_status;
mod mbc;
pub mod mmu;
pub mod model;
mod opcode;
pub mod ppu;
pub mod ram;
pub mod registers;
#[cfg(test)]
mod roms;
mod rtc;
pub mod savestate;
pub mod screenshot;
mod schedule;
mod serial;
pub mod sgb;
mod timer;

#[cfg(test)]
mod test_fixtures;
