fn main() {
    // `tauri::generate_context!` requires `frontendDist` to exist, even for
    // `cargo check`/`clippy`/`test` on a fresh clone that never built the
    // renderer. An empty folder is enough; `bun run build` fills it.
    let _ = std::fs::create_dir_all("../dist");
    tauri_build::build()
}
