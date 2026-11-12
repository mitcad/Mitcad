// SPDX-License-Identifier: MIT
//! `mitcad-thumbnailer <design.mitcad> <out.png> [<size>]` (mitcad#1539):
//! writes the design's preview as a PNG of at most `size` pixels a side
//! (256 when left out), for a file manager's thumbnailer entry
//! (`Exec=mitcad-thumbnailer %i %o %s`).
//!
//! Exit status: 0 when the thumbnail was written; 1, silently and with
//! nothing written, for a design without a preview (saved before MitCad
//! kept them: the file manager shows the file type's icon); 2 with a
//! message for anything else (no design file, a preview that cannot be
//! read, the output not writable, wrong arguments).

use std::process::ExitCode;

const USAGE: &str = "usage: mitcad-thumbnailer <design.mitcad> <out.png> [<size>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{USAGE}");
        println!(
            "Writes the preview a MitCad design file carries as a PNG of at most <size> \
             pixels a side (default {}).",
            mitcad_thumbnailer::DEFAULT_SIZE
        );
        println!("Exit status: 0 written, 1 the design has no preview, 2 an error.");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("mitcad-thumbnailer: {message}");
            ExitCode::from(2)
        }
    }
}

/// Writes the thumbnail; false for a design without a preview.
fn run(args: &[String]) -> Result<bool, String> {
    let (design, out, size) = match args {
        [design, out] => (design, out, mitcad_thumbnailer::DEFAULT_SIZE),
        [design, out, size] => {
            let size = size
                .parse::<u32>()
                .map_err(|_| format!("the size `{size}` is no number of pixels\n{USAGE}"))?;
            (design, out, size)
        }
        _ => return Err(USAGE.to_owned()),
    };
    let text = std::fs::read_to_string(design).map_err(|e| format!("cannot read {design}: {e}"))?;
    let Some(png) =
        mitcad_thumbnailer::thumbnail(&text, size).map_err(|e| format!("{design}: {e}"))?
    else {
        return Ok(false);
    };
    std::fs::write(out, png).map_err(|e| format!("cannot write {out}: {e}"))?;
    Ok(true)
}
