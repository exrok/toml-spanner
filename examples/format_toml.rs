use std::process::ExitCode;
use toml_spanner::{Arena, EmitConfig, emit_with_config, reproject};

fn main() -> ExitCode {
    let path = match std::env::args_os().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: format_toml <file.toml>");
            return ExitCode::FAILURE;
        }
    };

    let input = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", path.to_string_lossy());
            return ExitCode::FAILURE;
        }
    };

    // Parse the original document.
    let arena = Arena::new();
    let root = match toml_spanner::parse(&input, &arena) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // Clone the parsed tree into a second arena for the dest side of reproject.
    let dest_arena = Arena::new();
    let mut dest = root.table().clone_in(&dest_arena);

    // Reproject source formatting onto the cloned tree.
    let mut items = Vec::new();
    reproject(&root, &mut dest, &mut items);

    // Normalize and emit with format preservation + source ordering.
    let norm = dest.normalize();
    let config = EmitConfig {
        projected_source_text: &input,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit_with_config(norm, &config, &mut buf);

    print!(
        "{}",
        String::from_utf8(buf).expect("emit produced invalid UTF-8")
    );
    ExitCode::SUCCESS
}
