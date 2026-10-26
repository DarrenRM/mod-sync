fn main() {
    // Re-embed the exe and window icons whenever the icon files change (cargo doesn't
    // watch them on its own, so a rebuild would keep the old icon).
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
