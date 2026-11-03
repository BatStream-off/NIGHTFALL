//! Interface files embedded in the executable (served on nightfall://app/).

macro_rules! asset {
    ($name:literal, $mime:literal) => {
        ($name, include_bytes!(concat!("../../ui/", $name)) as &'static [u8], $mime)
    };
}

const HTML: &str = "text/html; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";
const SVG: &str = "image/svg+xml";

static ASSETS: &[(&str, &[u8], &str)] = &[
    asset!("shell.html", "text/html; charset=utf-8"),
    asset!("shell.css", "text/css; charset=utf-8"),
    asset!("shell.js", "text/javascript; charset=utf-8"),
    asset!("nf.css", "text/css; charset=utf-8"),
    asset!("nf.js", "text/javascript; charset=utf-8"),
    asset!("emblem.png", "image/png"),
    asset!("favicon.png", "image/png"),
    asset!("newtab.html", "text/html; charset=utf-8"),
    asset!("newtab.js", "text/javascript; charset=utf-8"),
    asset!("settings.html", "text/html; charset=utf-8"),
    asset!("settings.js", "text/javascript; charset=utf-8"),
    asset!("security.html", "text/html; charset=utf-8"),
    asset!("security.js", "text/javascript; charset=utf-8"),
    asset!("history.html", "text/html; charset=utf-8"),
    asset!("history.js", "text/javascript; charset=utf-8"),
    asset!("bookmarks.html", "text/html; charset=utf-8"),
    asset!("bookmarks.js", "text/javascript; charset=utf-8"),
    asset!("downloads.html", "text/html; charset=utf-8"),
    asset!("downloads.js", "text/javascript; charset=utf-8"),
    asset!("passwords.html", "text/html; charset=utf-8"),
    asset!("passwords.js", "text/javascript; charset=utf-8"),
    asset!("extensions.html", "text/html; charset=utf-8"),
    asset!("extensions.js", "text/javascript; charset=utf-8"),
    asset!("about.html", "text/html; charset=utf-8"),
    asset!("about.js", "text/javascript; charset=utf-8"),
];

pub fn get(path: &str) -> Option<(&'static [u8], &'static str)> {
    let _ = (HTML, CSS, JS, SVG);
    let path = if path.is_empty() { "newtab.html" } else { path };
    ASSETS.iter().find(|(n, _, _)| *n == path).map(|(_, b, m)| (*b, *m))
}
