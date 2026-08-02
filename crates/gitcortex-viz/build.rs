// Build script: ensures dist-viz/ exists so include_bytes! succeeds.
//
// Strategy: if dist-viz/ is missing, run `npm run build` in viz/. If npm is not
// available, write a placeholder so the Rust build does not fail catastrophically
// — the produced binary will serve a minimal "viz not built" page from the
// placeholder bytes. CI and release builds always have npm and produce the real
// bundle.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // viz/ was moved to workspace root (../../viz relative to this crate).
    let workspace_root = crate_dir.parent().unwrap().parent().unwrap();
    let viz_dir = workspace_root.join("viz");
    let dist_dir = crate_dir.join("dist-viz");
    let dist_assets = dist_dir.join("assets");

    // Paths relative to workspace root so cargo reruns on any viz change.
    println!("cargo:rerun-if-changed=../../viz/src");
    println!("cargo:rerun-if-changed=../../viz/index.html");
    println!("cargo:rerun-if-changed=../../viz/package.json");
    println!("cargo:rerun-if-changed=../../viz/vite.config.ts");
    println!("cargo:rerun-if-changed=../../viz/tsconfig.json");

    let needs_build = !dist_dir.join("index.html").exists()
        || !dist_assets.join("main.js").exists()
        || !dist_assets.join("main.css").exists()
        || !dist_assets.join("webgl-device.js").exists()
        || !dist_assets.join("CosmosCanvas.js").exists();

    if needs_build && viz_dir.exists() && which("npm").is_some() {
        let node_modules = viz_dir.join("node_modules");
        if !node_modules.exists() {
            println!("cargo:warning=Installing viz npm dependencies (one-time, ~30s)…");
            let status = Command::new("npm")
                .arg("install")
                .current_dir(&viz_dir)
                .status();
            if let Ok(s) = status {
                if !s.success() {
                    println!("cargo:warning=npm install failed; using placeholder viz assets");
                }
            }
        }
        if node_modules.exists() {
            println!("cargo:warning=Building viz frontend (npm run build)…");
            let status = Command::new("npm")
                .args(["run", "build"])
                .current_dir(&viz_dir)
                .status();
            if let Ok(s) = status {
                if !s.success() {
                    println!("cargo:warning=npm run build failed; using placeholder viz assets");
                }
            }
        }
    }

    ensure_placeholder(&dist_dir, &dist_assets);
}

fn ensure_placeholder(dist_dir: &Path, dist_assets: &Path) {
    let index = dist_dir.join("index.html");
    let js = dist_assets.join("main.js");
    let css = dist_assets.join("main.css");
    let webgl = dist_assets.join("webgl-device.js");
    let cosmos = dist_assets.join("CosmosCanvas.js");

    if index.exists() && js.exists() && css.exists() && webgl.exists() && cosmos.exists() {
        return;
    }

    std::fs::create_dir_all(dist_assets).ok();
    let placeholder_html = "<!doctype html><html><head><meta charset=\"utf-8\"><title>GitCortex Viz</title></head><body style=\"background:#06060a;color:#e6e6f0;font-family:monospace;padding:24px;\"><h2>Viz frontend not built</h2><p>Run <code>cd viz &amp;&amp; npm install &amp;&amp; npm run build</code> and rebuild <code>gcx</code>.</p></body></html>";

    if !index.exists() {
        std::fs::write(&index, placeholder_html).ok();
    }
    for f in [&js, &css, &webgl, &cosmos] {
        if !f.exists() {
            std::fs::write(f, "").ok();
        }
    }
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{cmd}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}
