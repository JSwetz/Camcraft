//! Identification embedded when the UI crate is compiled.

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const BUILT_UTC: &str = env!("CAMCRAFT_BUILD_UTC");
pub(crate) const SOURCE_SHA: &str = env!("CAMCRAFT_SOURCE_SHA");

// The toolbar label is pinned under test (version, build time and source)
// so the shell snapshots do not change with every version bump or commit.
// "0.0.0" is never a released version. The debug-info dialog keeps the real
// values.
#[cfg(test)]
const TOOLBAR_VERSION: &str = "0.0.0";
#[cfg(not(test))]
const TOOLBAR_VERSION: &str = VERSION;

#[cfg(test)]
const TOOLBAR_BUILD_UTC: &str = "2000-01-01 00:00 UTC";
#[cfg(not(test))]
const TOOLBAR_BUILD_UTC: &str = BUILT_UTC;

#[cfg(not(test))]
pub(crate) fn short_source() -> String {
    if SOURCE_SHA == "Custom Build" {
        SOURCE_SHA.to_owned()
    } else {
        let modified = if SOURCE_SHA.ends_with("-dirty") { "*" } else { "" };
        format!("{}{modified}", &SOURCE_SHA[..8])
    }
}

#[cfg(test)]
pub(crate) fn short_source() -> String {
    "test".to_owned()
}

/// The bare version label the toolbar shows at its narrowest width.
pub(crate) fn toolbar_version() -> String {
    format!("v{TOOLBAR_VERSION}")
}

pub(crate) fn toolbar_lines(compact: bool) -> (String, String) {
    let top = format!("{} · {}", toolbar_version(), short_source());
    let bottom = if compact {
        format!("{} UTC", TOOLBAR_BUILD_UTC.get(5..16).unwrap_or(TOOLBAR_BUILD_UTC))
    } else {
        format!("Built {TOOLBAR_BUILD_UTC}")
    };
    (top, bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_label_pins_the_test_version_not_the_package_version() {
        assert_eq!(toolbar_version(), "v0.0.0");
        assert_ne!(TOOLBAR_VERSION, env!("CARGO_PKG_VERSION"));
        for compact in [false, true] {
            let (top, _) = toolbar_lines(compact);
            assert_eq!(top, "v0.0.0 · test");
            assert!(!top.contains(env!("CARGO_PKG_VERSION")));
        }
    }
}
