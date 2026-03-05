use anyhow::{Context, Result};
use resvg::{tiny_skia, usvg};
use std::env;
use std::fs;
use std::path::Path;

fn render_svg_png(svg_bytes: &[u8], size: u32) -> Result<Vec<u8>> {
    let tree = usvg::Tree::from_data(svg_bytes, &usvg::Options::default())
        .context("failed to parse SVG data")?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).context("failed to allocate pixmap")?;
    let src_size = tree.size().to_int_size();
    let sx = size as f32 / src_size.width() as f32;
    let sy = size as f32 / src_size.height() as f32;
    let transform = tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().context("failed to encode PNG")
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        anyhow::bail!("Usage: render_svg_png <input.svg> <output.png> [size]");
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let size = args
        .get(3)
        .map(|s| s.parse::<u32>())
        .transpose()
        .context("size must be an integer")?
        .unwrap_or(1024);

    let svg_bytes = fs::read(input).with_context(|| format!("failed to read {}", input.display()))?;
    let png = render_svg_png(&svg_bytes, size)?;
    fs::write(output, png).with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}
