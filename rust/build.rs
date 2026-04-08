use std::env;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let vendor_dir = PathBuf::from(&manifest_dir)
        .join("../vendor/gmp")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&manifest_dir).join("../vendor/gmp"));

    if target.contains("apple-ios") {
        // iOS targets need cross-compiled GMP from vendor/
        let lib_dir = if target.contains("sim") {
            vendor_dir.join("ios-sim/lib")
        } else {
            vendor_dir.join("ios-device/lib")
        };

        if lib_dir.exists() {
            // Must appear early so the #[link(name = "gmp")] in rust-gmp-kzen finds it
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
        } else {
            println!("cargo:warning=GMP vendor lib not found at {}, falling back to system", lib_dir.display());
        }
    } else if target.contains("apple-darwin") {
        // macOS host — use Homebrew GMP
        if let Ok(prefix) = env::var("HOMEBREW_PREFIX") {
            println!("cargo:rustc-link-search=native={}/lib", prefix);
        } else {
            // Default Homebrew paths
            println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
            println!("cargo:rustc-link-search=native=/usr/local/lib");
        }
    } else if target.contains("android") {
        // Android: GMP needs to be cross-compiled or use num-bigint
        // For now, let the CI handle this via pre-compiled GMP or environment vars
        if let Ok(gmp_dir) = env::var("GMP_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", gmp_dir);
        }
    }
}
