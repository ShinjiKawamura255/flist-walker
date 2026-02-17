use std::env;
use std::fs::File;
use std::path::PathBuf;

use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
use resvg::{tiny_skia, usvg};

fn render_svg_icon_rgba(size: u32) -> Option<Vec<u8>> {
    let svg = include_bytes!("assets/flistwalker-icon.svg");
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let src_size = tree.size().to_int_size();
    let sx = size as f32 / src_size.width() as f32;
    let sy = size as f32 / src_size.height() as f32;
    let transform = tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap.data().to_vec())
}

fn main() {
    println!("cargo:rerun-if-changed=assets/flistwalker-icon.svg");

    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let Some(rgba) = render_svg_icon_rgba(256) else {
        panic!("failed to render SVG icon for Windows resource");
    };

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let ico_path = out_dir.join("flistwalker.ico");
    let image = IconImage::from_rgba_data(256, 256, rgba);
    let entry = IconDirEntry::encode(&image).expect("encode ico entry");
    let mut icon_dir = IconDir::new(ResourceType::Icon);
    icon_dir.add_entry(entry);
    let mut file = File::create(&ico_path).expect("create .ico");
    icon_dir.write(&mut file).expect("write .ico");

    let host = env::var("HOST").unwrap_or_default();
    let has_explicit_rc = env::var("RC").is_ok();
    if host.contains("windows") || has_explicit_rc {
        let mut res = winres::WindowsResource::new();
        res.set_icon(ico_path.to_str().expect("ico path utf-8"));
        if let Err(err) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon: {err}");
        }
    } else {
        println!(
            "cargo:warning=skipping Windows EXE icon embedding on non-Windows host \
             (set RC env var to enable)"
        );
    }
}
