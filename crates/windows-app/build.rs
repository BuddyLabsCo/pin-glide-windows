#[cfg(windows)]
fn main() -> Result<(), slint_build::CompileError> {
    println!("cargo:rerun-if-env-changed=SLINT_COMPILER_DENY_WARNINGS");
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles),
    )
}

#[cfg(not(windows))]
fn main() {}
