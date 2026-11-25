use url::Url;

#[path = "src/service/origin.rs"]
mod origin;

fn validate_service_origin() {
    println!("cargo:rerun-if-env-changed=PINGLIDE_SERVICE_URL");
    if let Some(value) = std::env::var_os("PINGLIDE_SERVICE_URL") {
        assert!(
            value.to_str().and_then(origin::parse).is_some(),
            "Set PINGLIDE_SERVICE_URL to a valid Service origin for this transport mode."
        );
    }
}

#[cfg(windows)]
fn main() -> Result<(), slint_build::CompileError> {
    validate_service_origin();
    println!("cargo:rerun-if-env-changed=SLINT_COMPILER_DENY_WARNINGS");
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles),
    )
}

#[cfg(not(windows))]
fn main() {
    validate_service_origin();
}
