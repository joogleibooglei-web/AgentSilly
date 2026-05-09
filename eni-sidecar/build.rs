/// Build script that embeds the git tag version into the binary at compile time.
/// Falls back to CARGO_PKG_VERSION if no git tag is available.
use std::process::Command;

fn main() {
    // Try to get the version from the latest git tag
    let version = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
                // Strip leading 'v' if present
                Some(tag.strip_prefix('v').unwrap_or(&tag).to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=ENI_VERSION={}", version);
    // Re-run if git HEAD changes (new tag/commit)
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/tags");
}
