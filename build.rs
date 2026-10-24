//! The build number for About: the commit the binary was built from.
fn main() {
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "dev".into());
    println!("cargo:rustc-env=LIBRI_BUILD={hash}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
