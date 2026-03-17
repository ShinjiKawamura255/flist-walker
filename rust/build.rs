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

fn find_program_in_path(candidates: &[&str]) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        for candidate in candidates {
            let full = dir.join(candidate);
            if full.is_file() {
                return Some(full);
            }
            #[cfg(windows)]
            {
                let exe = dir.join(format!("{candidate}.exe"));
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=assets/flistwalker-icon.svg");

    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let ico_path = out_dir.join("flistwalker.ico");
    let mut icon_dir = IconDir::new(ResourceType::Icon);
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let Some(rgba) = render_svg_icon_rgba(size) else {
            panic!("failed to render SVG icon for Windows resource at {size}px");
        };
        let image = IconImage::from_rgba_data(size, size, rgba);
        let entry = IconDirEntry::encode(&image).expect("encode ico entry");
        icon_dir.add_entry(entry);
    }
    let mut file = File::create(&ico_path).expect("create .ico");
    icon_dir.write(&mut file).expect("write .ico");

    let host = env::var("HOST").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "gnu" && !host.contains("windows") {
        let windres = env::var_os("FLISTWALKER_WINDOWS_WINDRES")
            .map(PathBuf::from)
            .or_else(|| find_program_in_path(&["x86_64-w64-mingw32-windres", "windres"]));
        let ar = env::var_os("FLISTWALKER_WINDOWS_AR")
            .map(PathBuf::from)
            .or_else(|| find_program_in_path(&["x86_64-w64-mingw32-ar", "ar"]));
        if let (Some(windres), Some(ar)) = (windres, ar) {
            let mut res = winres::WindowsResource::new();
            res.set_icon(ico_path.to_str().expect("ico path utf-8"));
            res.set_windres_path(windres.to_str().expect("windres path utf-8"));
            res.set_ar_path(ar.to_str().expect("ar path utf-8"));
            res.compile()
                .expect("failed to embed Windows icon resource with windres");
        } else {
            println!(
                "cargo:warning=skipping Windows EXE icon embedding on non-Windows GNU host \
                 (install x86_64-w64-mingw32-windres and x86_64-w64-mingw32-ar or set FLISTWALKER_WINDOWS_WINDRES / \
                 FLISTWALKER_WINDOWS_AR)"
            );
        }
        return;
    }

    let rc_path = env::var_os("RC")
        .map(PathBuf::from)
        .or_else(|| find_program_in_path(&["llvm-rc"]));
    if let Some(rc) = rc_path.as_ref() {
        env::set_var("RC", rc);
    }
    if host.contains("windows") || rc_path.is_some() {
        let mut res = winres::WindowsResource::new();
        res.set_icon(ico_path.to_str().expect("ico path utf-8"));
        res.compile()
            .expect("failed to embed Windows icon resource");
    } else {
        println!(
            "cargo:warning=skipping Windows EXE icon embedding on non-Windows host \
             (install llvm-rc or set RC env var to enable)"
        );
    }
}
