mod icon_render {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/icon_render.rs"));
}

fn main() {
    println!("cargo:rerun-if-changed=icon_render.rs");

    write_shared_icon_assets();

    #[cfg(windows)]
    write_windows_icon_resources();
}

/// 与 Windows .exe 共用 icon_render.rs，供 macOS .icns / 文档引用。
fn write_shared_icon_assets() {
    use std::path::PathBuf;

    let assets_dir = PathBuf::from("assets");
    std::fs::create_dir_all(&assets_dir).expect("create assets dir");

    let png_path = assets_dir.join("claude-code-helper.png");
    let rgba = icon_render::render_icon_rgba(512);
    write_png(&png_path, 512, &rgba).expect("write claude-code-helper.png");
}

fn write_png(path: &std::path::Path, size: u32, rgba: &[u8]) -> Result<(), png::EncodingError> {
    use png::{BitDepth, ColorType, Encoder};
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = Encoder::new(writer, size, size);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut png_writer = encoder.write_header()?;
    png_writer.write_image_data(rgba)?;
    Ok(())
}

#[cfg(windows)]
fn write_windows_icon_resources() {
    use std::path::PathBuf;

    let ico_bytes = generate_icon_dir_bytes();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico_out = out_dir.join("app.ico");
    std::fs::write(&ico_out, &ico_bytes).expect("write app.ico");

    let assets_dir = PathBuf::from("assets");
    std::fs::write(assets_dir.join("claude-code-helper.ico"), &ico_bytes).ok();

    let mut res = winres::WindowsResource::new();
    res.set_icon(ico_out.to_str().expect("icon path utf-8"));
    res.set("ProductName", "Claude Code Helper");
    res.set("FileDescription", "Claude Code Helper");
    res.set("LegalCopyright", "Claude Code Helper");
    res.set("FileVersion", env!("CARGO_PKG_VERSION"));
    res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
    res.compile().expect("compile windows resources");
}

#[cfg(windows)]
fn generate_icon_dir_bytes() -> Vec<u8> {
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let rgba = icon_render::render_icon_rgba(size);
        let image = ico::IconImage::from_rgba_data(size, size, rgba);
        icon_dir
            .add_entry(ico::IconDirEntry::encode(&image).expect("encode icon entry"));
    }
    let mut bytes = Vec::new();
    icon_dir.write(&mut bytes).expect("write icon dir");
    bytes
}
