use std::{io, path::PathBuf};

const DIRECTORY: &str = "PINGLIDE_TEST_DIRECTORY";

pub(crate) fn validate() -> io::Result<()> {
    if cfg!(debug_assertions)
        && std::env::var_os(DIRECTORY).is_some()
        && !directory().is_some_and(|path| path.is_dir())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PINGLIDE_TEST_DIRECTORY must be an existing absolute PinGlideTest-* directory",
        ));
    }
    Ok(())
}

pub(crate) fn directory() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let path = PathBuf::from(std::env::var_os(DIRECTORY)?);
    (path.is_absolute() && name(&path).is_some()).then_some(path)
}

pub(crate) fn name(path: &std::path::Path) -> Option<&str> {
    path.file_name()?.to_str().filter(|name| {
        name.strip_prefix("PinGlideTest-")
            .is_some_and(|suffix| !suffix.is_empty())
    })
}
