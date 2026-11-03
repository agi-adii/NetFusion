fn main() {
    tauri_build::build();

    #[cfg(windows)]
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let out = std::path::Path::new(&out_dir);
        for candidate in ["resource.lib", "libresource.a", "resource.o"] {
            let p = out.join(candidate);
            if p.exists() {
                println!("cargo:rustc-link-arg-tests={}", p.display());
                break;
            }
        }
    }
}
