//! Write the screen each `.pkrd` save carries beside it as a `.png`, for the story exporter.

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("could not read {path}: {e}"));
        match pokered::saved_screenshot(&bytes) {
            Ok(Some(png)) => {
                let out = std::path::Path::new(&path).with_extension("png");
                std::fs::write(&out, png).unwrap_or_else(|e| panic!("could not write {}: {e}", out.display()));
            }
            Ok(None) => eprintln!("{path}: no screen"),
            Err(e) => eprintln!("{path}: {e}"),
        }
    }
}
