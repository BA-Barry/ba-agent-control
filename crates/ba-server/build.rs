use std::{
    env,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=../../apps/console/dist");
    println!("cargo:rerun-if-changed=static/fallback.html");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(io::Error::other)?);
    let console_dist = manifest.join("../../apps/console/dist");
    let source = if console_dist.join("index.html").is_file() {
        console_dist
    } else {
        manifest.join("static")
    };

    let mut assets = Vec::new();
    collect_assets(&source, &source, &mut assets)?;
    assets.sort_by(|left, right| left.0.cmp(&right.0));

    let mut generated = String::from(
        "fn embedded_asset(path: &str) -> Option<(&'static [u8], &'static str)> {\n    match path {\n",
    );
    for (relative, absolute) in assets {
        let route = if relative == "fallback.html" {
            "index.html"
        } else {
            &relative
        };
        let escaped = absolute.to_string_lossy().replace('\\', "\\\\");
        writeln!(
            generated,
            "        {route:?} => Some((include_bytes!(\"{escaped}\"), {mime:?})),",
            mime = mime_for(route),
        )
        .map_err(io::Error::other)?;
    }
    generated.push_str("        _ => None,\n    }\n}\n");
    let output =
        PathBuf::from(env::var("OUT_DIR").map_err(io::Error::other)?).join("embedded_assets.rs");
    fs::write(output, generated)
}

fn collect_assets(
    root: &Path,
    directory: &Path,
    assets: &mut Vec<(String, PathBuf)>,
) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_assets(root, &path, assets)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_string_lossy()
                .replace('\\', "/");
            assets.push((relative, path.canonicalize()?));
        }
    }
    Ok(())
}

fn mime_for(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
