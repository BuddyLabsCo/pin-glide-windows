use std::path::PathBuf;

pub(crate) fn directory() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let path = PathBuf::from(std::env::var_os("PINGLIDE_TEST_DIRECTORY")?);
    (path.is_absolute() && name(&path).is_some()).then_some(path)
}

pub(crate) fn name(path: &std::path::Path) -> Option<&str> {
    path.file_name()?
        .to_str()
        .filter(|name| name.starts_with("PinGlideTest-"))
}
