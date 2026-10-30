//! Renders a score file to PNG, SVG, or PDF without starting the app, for checking layout.
//!
//! `cargo run --manifest-path src-tauri/Cargo.toml --example render -- <score> <output> [width]`
//! reads `.notula`, `.musicxml`, or `.mxl` files; the output format follows its extension and
//! the width is in staff spaces (the export default is 80).

use std::path::Path;

use notula_app_lib::export::{ExportFormat, export_render_list};
use notula_layout::LayoutOptions;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, rest @ ..] = &args[..] else {
        return Err("usage: render <score> <output.png|svg|pdf> [width]".to_owned());
    };
    let width = rest
        .first()
        .map(|width| width.parse::<f32>().map_err(|error| error.to_string()))
        .transpose()?
        .unwrap_or(80.0);

    let input = Path::new(input);
    let score = match input.extension().and_then(|extension| extension.to_str()) {
        Some("musicxml" | "xml" | "mxl") => {
            let imported =
                notula_format::musicxml::import_file(input).map_err(|error| error.to_string())?;
            for warning in &imported.unsupported {
                eprintln!("unsupported: {warning}");
            }
            imported.score
        }
        _ => notula_format::load(input).map_err(|error| error.to_string())?,
    };
    let output = Path::new(output);
    let format = ExportFormat::parse(
        output
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default(),
    )?;
    let list = notula_layout::layout(&score, &LayoutOptions { width });
    export_render_list(&list, format, output)
}
