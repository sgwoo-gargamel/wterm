use std::process::Command;

/// Short git revision, with `+` appended when the working tree has changes
fn git_revision() -> String {
    let rev = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    if rev.is_empty() {
        return String::new();
    }
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| !out.stdout.is_empty())
        .unwrap_or(false);
    if dirty {
        format!("{rev}+")
    } else {
        rev
    }
}

fn main() {
    println!("cargo:rustc-env=WTERM_GIT_REV={}", git_revision());
    // Pick up new commits without a manual clean
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads");
    tauri_build::build()
}
