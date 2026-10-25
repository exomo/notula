use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use notula_layout::{Anchor, RenderList, Shape};
use resvg::usvg;
use resvg::{tiny_skia, usvg::fontdb::Database};

const BRAVURA_TTF: &[u8] = include_bytes!("../../ui/src/fonts/Bravura.ttf");

#[derive(Clone, Copy)]
pub enum ExportFormat {
    Pdf,
    Png,
    Svg,
}

impl ExportFormat {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "pdf" => Ok(Self::Pdf),
            "png" => Ok(Self::Png),
            "svg" => Ok(Self::Svg),
            _ => Err(format!("Unsupported export format: {value}")),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }

    pub fn filter_name(self) -> &'static str {
        match self {
            Self::Pdf => "PDF document",
            Self::Png => "PNG image",
            Self::Svg => "SVG image",
        }
    }
}

pub fn export_render_list(
    list: &RenderList,
    format: ExportFormat,
    path: &Path,
) -> Result<(), String> {
    std::fs::write(path, render_bytes(list, format)?).map_err(|error| error.to_string())
}

fn render_bytes(list: &RenderList, format: ExportFormat) -> Result<Vec<u8>, String> {
    let svg = render_list_svg(list);
    match format {
        ExportFormat::Svg => Ok(svg.into_bytes()),
        ExportFormat::Pdf => {
            let tree = parse_svg(&svg)?;
            svg2pdf::to_pdf(
                &tree,
                svg2pdf::ConversionOptions::default(),
                svg2pdf::PageOptions::default(),
            )
            .map_err(|error| error.to_string())
        }
        ExportFormat::Png => {
            let tree = parse_svg(&svg)?;
            let scale = 4.0_f32;
            let width = (tree.size().width() * scale).ceil() as u32;
            let height = (tree.size().height() * scale).ceil() as u32;
            let mut pixmap = tiny_skia::Pixmap::new(width, height)
                .ok_or_else(|| "Could not allocate the PNG image.".to_owned())?;
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            pixmap.encode_png().map_err(|error| error.to_string())
        }
    }
}

fn parse_svg(svg: &str) -> Result<usvg::Tree, String> {
    let mut options = usvg::Options::default();
    let mut fonts = Database::new();
    fonts.load_font_data(BRAVURA_TTF.to_vec());
    options.fontdb = Arc::new(fonts);
    usvg::Tree::from_str(svg, &options).map_err(|error| error.to_string())
}

fn render_list_svg(list: &RenderList) -> String {
    let font = base64::engine::general_purpose::STANDARD.encode(BRAVURA_TTF);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\"><style>@font-face{{font-family:Bravura;src:url(data:font/ttf;base64,{font}) format('truetype')}}text{{fill:#000}}.glyph{{font-family:Bravura}}</style><rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
        list.width * 8.0,
        list.height * 8.0,
        list.width,
        list.height
    );
    for item in &list.items {
        match &item.shape {
            Shape::Line {
                x1,
                y1,
                x2,
                y2,
                thickness,
            } => svg.push_str(&format!(
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"black\" stroke-width=\"{thickness}\"/>"
            )),
            Shape::Polygon { points } => {
                let points = points
                    .iter()
                    .map(|point| format!("{},{}", point[0], point[1]))
                    .collect::<Vec<_>>()
                    .join(" ");
                svg.push_str(&format!("<polygon points=\"{points}\" fill=\"black\"/>"));
            }
            Shape::Glyph { x, y, glyph, size } => svg.push_str(&format!(
                "<text class=\"glyph\" x=\"{x}\" y=\"{y}\" font-size=\"{size}\">&#x{:X};</text>",
                *glyph as u32
            )),
            Shape::Text {
                x,
                y,
                text,
                size,
                anchor,
            } => {
                let anchor = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                svg.push_str(&format!(
                    "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" text-anchor=\"{anchor}\">{}</text>",
                    escape_xml(text)
                ));
            }
        }
    }
    svg.push_str("</svg>");
    svg
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_export_embeds_bravura_and_escapes_text() {
        let list = RenderList {
            width: 20.0,
            height: 10.0,
            items: vec![
                notula_layout::Item {
                    event: None,
                    staff: None,
                    voice: None,
                    shape: Shape::Text {
                        x: 1.0,
                        y: 2.0,
                        text: "A & B".to_owned(),
                        size: 2.0,
                        anchor: Anchor::Start,
                    },
                },
                notula_layout::Item {
                    event: None,
                    staff: None,
                    voice: None,
                    shape: Shape::Glyph {
                        x: 10.0,
                        y: 8.0,
                        glyph: '\u{E0A4}',
                        size: 4.0,
                    },
                },
            ],
            areas: Vec::new(),
        };
        let svg = render_list_svg(&list);
        assert!(svg.contains("data:font/ttf;base64,"));
        assert!(svg.contains("A &amp; B"));
        assert!(parse_svg(&svg).is_ok());
        assert!(
            render_bytes(&list, ExportFormat::Pdf)
                .unwrap()
                .starts_with(b"%PDF-")
        );
        assert!(
            render_bytes(&list, ExportFormat::Png)
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
        let tree = parse_svg(&svg).unwrap();
        let scale = 4.0;
        let mut pixmap = tiny_skia::Pixmap::new(
            (tree.size().width() * scale).ceil() as u32,
            (tree.size().height() * scale).ceil() as u32,
        )
        .unwrap();
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        assert!(pixmap.data().chunks_exact(4).any(|pixel| pixel[0] < 250));
    }
}
