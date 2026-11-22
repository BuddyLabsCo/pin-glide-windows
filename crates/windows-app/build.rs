#[cfg(windows)]
fn main() -> Result<(), slint_build::CompileError> {
    println!("cargo:rerun-if-env-changed=SLINT_COMPILER_DENY_WARNINGS");
    slint_build::compile("ui/app.slint")
}

#[cfg(not(windows))]
fn main() {}
