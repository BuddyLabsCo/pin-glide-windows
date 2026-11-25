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
    embed_windows_resources();
    println!("cargo:rerun-if-env-changed=SLINT_COMPILER_DENY_WARNINGS");
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles),
    )
}

#[cfg(windows)]
fn embed_windows_resources() {
    use std::{env, fs, path::PathBuf};

    let icon = "ui/assets/app-icon.ico";
    println!("cargo:rerun-if-changed={icon}");
    let target = env::var("TARGET").unwrap();
    let rc = find_msvc_tools::find_windows_sdk(&target)
        .and_then(|sdk| {
            sdk.path()
                .map(|path| path.join("rc.exe"))
                .find(|path| path.is_file())
        })
        .or_else(|| {
            let sdk = PathBuf::from(env::var_os("ProgramFiles(x86)")?).join("Windows Kits/10/bin");
            fs::read_dir(sdk)
                .ok()?
                .filter_map(Result::ok)
                .map(|entry| entry.path().join("x64/rc.exe"))
                .filter(|path| path.is_file())
                .max()
        })
        .unwrap_or_else(|| PathBuf::from("rc.exe"));
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let source = out.join("receiver.rc");
    let resource = out.join("receiver.res");
    let icon = env::current_dir().unwrap().join(icon);
    let icon = icon.to_str().unwrap().replace('\\', "\\\\");
    let version = env!("CARGO_PKG_VERSION");
    let fixed = format!(
        "{},{},{},0",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    );
    let mut script = format!("LANGUAGE 9, 1\n1 ICON \"{icon}\"\n");
    let english_fallbacks = (1..=0x03ff)
        .filter(|&primary| primary != 4)
        .map(|primary| (primary, 0, "Pin Glide"));
    for (primary, sublanguage, name) in english_fallbacks.chain([
        (9, 1, "Pin Glide"),
        (4, 0, "鹊信"),
        (4, 1, "鹊信"),
        (4, 2, "鹊信"),
        (4, 3, "鹊信"),
        (4, 4, "鹊信"),
        (4, 5, "鹊信"),
        (4, 30, "鹊信"),
        (4, 31, "鹊信"),
    ]) {
        let lang = primary | (sublanguage << 10);
        script.push_str(&format!(
            r#"
LANGUAGE {primary}, {sublanguage}
STRINGTABLE
BEGIN
    101 "{name}"
END
1 VERSIONINFO
FILEVERSION {fixed}
PRODUCTVERSION {fixed}
FILEOS 0x40004
FILETYPE 1
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "{lang:04x}04b0"
        BEGIN
            VALUE "CompanyName", "BuddyLabsCo"
            VALUE "ProductName", "{name}"
            VALUE "FileDescription", "{name}"
            VALUE "OriginalFilename", "windows-app.exe"
            VALUE "FileVersion", "{version}"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", {lang}, 1200
    END
END
"#
        ));
    }
    fs::write(&source, script).unwrap();
    let status = std::process::Command::new(rc)
        .args(["/nologo", "/c65001", "/fo"])
        .arg(&resource)
        .arg(&source)
        .status()
        .expect("run Windows resource compiler");
    assert!(status.success(), "Windows resource compilation failed");
    println!(
        "cargo:rustc-link-arg-bin=windows-app={}",
        resource.display()
    );
}

#[cfg(not(windows))]
fn main() {
    validate_service_origin();
}
