#[cfg(windows)]
mod icon_render {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/icon_render.rs"));
}

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;

    let ico_bytes = generate_icon_dir_bytes();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico_out = out_dir.join("app.ico");
    std::fs::write(&ico_out, &ico_bytes).expect("write app.ico");

    let assets_dir = PathBuf::from("assets");
    std::fs::create_dir_all(&assets_dir).ok();
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

#[cfg(not(windows))]
fn main() {}

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
