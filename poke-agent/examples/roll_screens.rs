//! Load a `.pkrd` save and play it forward, writing the screen at each point worth a picture, for
//! the story exporter to choose from.
//!
//!     roll_screens <save.pkrd> <out-prefix> [--card] [--until TEXT] [--seconds N]
//!
//! Writes `<out-prefix>-NN.png` and one JSON line per picture on stdout: `{file, frame, why, text}`.
//! A picture is taken whenever a message waits for its `▼` (and then advanced), every second while
//! nothing waits (`why: "tick"`), and, with `--card`, of the trainer card once play is free, which
//! is also where it stops. `--until` stops at the first picture whose message contains TEXT.
//! Any other decision is answered with A, so a battle is fought with its first move.

use gbc::lcd_palette::LcdColor;
use poke_agent::pokemon::native::message_box_text;
use pokered::command::{Command, Decision};
use pokered::gfx::colour::ColourMode;
use pokered::input::Joypad;
use pokered::mode::Status;
use pokered::modes::start_menu::StartMenuEntry;
use pokered::{Game, Input, Pacing};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 144;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (save, prefix) = (&args[0], &args[1]);
    let card = args.iter().any(|a| a == "--card");
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].clone());
    let seconds: u64 = flag("--seconds").map_or(90, |s| s.parse().unwrap());
    let until = flag("--until");

    let bytes = std::fs::read(save).unwrap_or_else(|e| panic!("could not read {save}: {e}"));
    let mut game = Game::load(&bytes, Pacing::Faithful).unwrap_or_else(|e| panic!("{save}: {e}"));
    let mut shots = 0;
    // Whether the picture just taken is the one `--until` asked for.
    let mut shoot = |game: &Game, why: &str| -> bool {
        shots += 1;
        let file = format!("{prefix}-{shots:02}.png");
        let rgba = ColourMode::Gbc.rgba(game.screen());
        let pixels: Vec<LcdColor> = rgba.chunks_exact(4).map(|p| LcdColor::rgb(p[0], p[1], p[2])).collect();
        std::fs::write(&file, gbc::screenshot::png(&pixels, WIDTH, HEIGHT)).unwrap();
        let text = message_box_text(game.ui());
        println!("{}", serde_json::json!({ "file": file, "frame": game.frames(), "why": why, "text": text.trim() }));
        until.as_deref().is_some_and(|until| text.contains(until))
    };

    let mut pressed = false;
    let mut carded = false;
    for frame in 0..seconds * 60 {
        let input = match game.status() {
            Status::Waiting(Decision::Text) => {
                if shoot(&game, "text") {
                    break;
                }
                Input::Command(Command::Advance)
            }
            Status::Waiting(Decision::TrainerCard) => {
                shoot(&game, "card");
                break;
            }
            // Through the start menu, which loads the font the card's labels are drawn in.
            Status::Idle | Status::Waiting(Decision::Overworld) if card && !carded => {
                carded = true;
                Input::Command(Command::OpenStartMenu)
            }
            Status::Waiting(Decision::StartMenu) if card => Input::Command(Command::ChooseStartMenuEntry(StartMenuEntry::TrainerInfo)),
            // Play is free and nothing is happening: the event is over.
            Status::Idle | Status::Waiting(Decision::Overworld) if frame > 120 => break,
            Status::Waiting(_) => {
                // A tap, not a hold: a held A is one press.
                pressed = !pressed;
                Input::Buttons(if pressed { Joypad::A } else { Joypad::empty() })
            }
            _ => {
                if frame % 60 == 0 && shoot(&game, "tick") {
                    break;
                }
                Input::None
            }
        };
        game.frame(input);
    }
}
