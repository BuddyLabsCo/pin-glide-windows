use super::Url;

pub(super) fn parse(value: &str) -> Option<Url> {
    let origin = Url::parse(value).ok()?;
    if (origin.scheme() != "https"
        && !(cfg!(feature = "development-transport") && origin.scheme() == "http"))
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return None;
    }
    Some(origin)
}
