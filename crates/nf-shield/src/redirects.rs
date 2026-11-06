//! Neutral replacements served instead of a blocked request when a filter
//! asks for `$redirect=…` (or `$redirect-rule=…`): an empty script, a
//! transparent image, a silent sound… A page then gets an answer that does
//! nothing instead of a network error, so it breaks less and is less able
//! to notice the blocker.
//!
//! Original content (MPL-2.0): trivial files written or generated for
//! NIGHTFALL (`redirects/`), under the resource names used by public filter
//! lists. No uBlock Origin resource is included.

use adblock::resources::{MimeType, PermissionMask, Resource, ResourceType};
use base64::{engine::general_purpose::STANDARD, Engine as _};

struct Def {
    name: &'static str,
    aliases: &'static [&'static str],
    mime: MimeType,
    body: &'static [u8],
}

fn defs() -> Vec<Def> {
    vec![
        Def { name: "noop.js", aliases: &["noopjs", "abp-resource:blank-js", "blank-js"], mime: MimeType::ApplicationJavascript, body: b"(function () {})();\n" },
        Def { name: "noop.txt", aliases: &["nooptext", "abp-resource:blank-text", "empty"], mime: MimeType::TextPlain, body: b"" },
        Def { name: "noop.html", aliases: &["noopframe", "abp-resource:blank-html"], mime: MimeType::TextHtml, body: b"<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body></body></html>\n" },
        Def { name: "noop.css", aliases: &["abp-resource:blank-css"], mime: MimeType::TextCss, body: b"" },
        Def { name: "noop.json", aliases: &["noopjson"], mime: MimeType::ApplicationJson, body: b"{}" },
        Def { name: "1x1.gif", aliases: &["1x1-transparent.gif", "1x1-transparent-gif", "abp-resource:1x1-transparent-gif"], mime: MimeType::ImageGif, body: include_bytes!("../redirects/1x1.gif") },
        Def { name: "2x2.png", aliases: &["2x2-transparent.png", "abp-resource:2x2-transparent-png"], mime: MimeType::ImagePng, body: include_bytes!("../redirects/2x2.png") },
        Def { name: "3x2.png", aliases: &["3x2-transparent.png", "abp-resource:3x2-transparent-png"], mime: MimeType::ImagePng, body: include_bytes!("../redirects/3x2.png") },
        Def { name: "32x32.png", aliases: &["32x32-transparent.png", "abp-resource:32x32-transparent-png"], mime: MimeType::ImagePng, body: include_bytes!("../redirects/32x32.png") },
        Def { name: "noop-0.1s.mp3", aliases: &["noopmp3-0.1s", "abp-resource:blank-mp3", "blank-mp3"], mime: MimeType::AudioMp3, body: include_bytes!("../redirects/noop-0.1s.mp3") },
        Def { name: "noop-1s.mp4", aliases: &["noopmp4-1s", "abp-resource:blank-mp4", "blank-mp4"], mime: MimeType::VideoMp4, body: include_bytes!("../redirects/noop-1s.mp4") },
        Def { name: "noop-vmap1.0.xml", aliases: &["noop-vmap1.xml", "noopvmap-1.0"], mime: MimeType::TextXml, body: b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><vmap:VMAP xmlns:vmap=\"http://www.iab.net/videosuite/vmap\" version=\"1.0\"></vmap:VMAP>" },
        Def { name: "noopvast-2.0", aliases: &["noopvast-2.0.xml"], mime: MimeType::TextXml, body: b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><VAST version=\"2.0\"></VAST>" },
        Def { name: "noopvast-3.0", aliases: &["noopvast-3.0.xml"], mime: MimeType::TextXml, body: b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><VAST version=\"3.0\"></VAST>" },
        Def { name: "noopvast-4.0", aliases: &["noopvast-4.0.xml"], mime: MimeType::TextXml, body: b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><VAST version=\"4.0\"></VAST>" },
    ]
}

/// Redirect resources, ready for `Engine::use_resources` (with the scriptlets).
pub fn resources() -> Vec<Resource> {
    defs()
        .into_iter()
        .map(|d| Resource {
            name: d.name.to_string(),
            aliases: d.aliases.iter().map(|a| a.to_string()).collect(),
            kind: ResourceType::Mime(d.mime),
            content: STANDARD.encode(d.body),
            dependencies: Vec::new(),
            permission: PermissionMask::default(),
        })
        .collect()
}

/// `data:<mime>;base64,<body>` (what the engine gives for a redirect) →
/// (MIME type, bytes). Anything else is refused.
pub fn decode_data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (mime, b64) = rest.split_once(";base64,")?;
    if mime.is_empty() || mime.len() > 60 || !mime.chars().all(|c| c.is_ascii_alphanumeric() || "/+-.".contains(c)) {
        return None;
    }
    let body = STANDARD.decode(b64).ok()?;
    (body.len() <= 64 * 1024).then(|| (mime.to_string(), body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_and_media_are_real_files() {
        let r = resources();
        let get = |n: &str| STANDARD.decode(&r.iter().find(|x| x.name == n).unwrap().content).unwrap();
        assert!(get("1x1.gif").starts_with(b"GIF89a"));
        for p in ["2x2.png", "3x2.png", "32x32.png"] {
            assert!(get(p).starts_with(b"\x89PNG\r\n\x1a\n"), "{p}");
        }
        let mp3 = get("noop-0.1s.mp3");
        assert!(mp3[0] == 0xFF && (mp3[1] & 0xE0) == 0xE0, "trame MP3 attendue");
        assert_eq!(&get("noop-1s.mp4")[4..8], b"ftyp");
        // Names are unique, aliases included.
        let mut all: Vec<&str> = r.iter().flat_map(|x| std::iter::once(x.name.as_str()).chain(x.aliases.iter().map(|a| a.as_str()))).collect();
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
    }

    #[test]
    fn data_urls_are_read_strictly() {
        assert_eq!(decode_data_url("data:text/plain;base64,"), Some(("text/plain".into(), vec![])));
        assert_eq!(decode_data_url("data:application/javascript;base64,KGZ1bmN0aW9uICgpIHt9KSgpOwo=").unwrap().1, b"(function () {})();\n");
        assert!(decode_data_url("https://x.example/").is_none());
        assert!(decode_data_url("data:text/html,<script>").is_none());
        assert!(decode_data_url("data:text/html\";x;base64,AA==").is_none());
    }
}
