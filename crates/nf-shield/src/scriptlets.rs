//! NIGHTFALL's own scriptlet resources for `##+js(...)` filters.
//!
//! Original implementations (MPL-2.0) of the scriptlet names used by
//! public filter lists. No uBlock Origin code is included.

use adblock::resources::{MimeType, PermissionMask, Resource, ResourceType};
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Permission bit for "trusted" scriptlets (uBO's `requiresTrust`).
pub const TRUSTED: PermissionMask = PermissionMask::from_bits(0b0000_0001);

struct Def {
    name: &'static str,
    aliases: &'static [&'static str],
    trusted: bool,
    source: &'static str,
}

macro_rules! def {
    ($file:literal, [$($alias:literal),*], $trusted:expr) => {
        Def { name: $file, aliases: &[$($alias),*], trusted: $trusted, source: include_str!(concat!("../scriptlets/", $file)) }
    };
}

const UTILS: &str = include_str!("../scriptlets/nf-utils.fn.js");

fn defs() -> Vec<Def> {
    vec![
        def!("set-constant.js", ["set.js", "set-constant"], false),
        def!("json-prune.js", [], false),
        def!("json-prune-fetch-response.js", ["jpfr.js"], false),
        def!("json-prune-xhr-response.js", ["jpxr.js"], false),
        def!("trusted-replace-fetch-response.js", ["trusted-rpfr.js"], true),
        def!("trusted-replace-xhr-response.js", ["trusted-rpxr.js"], true),
        def!("abort-on-property-read.js", ["aopr.js"], false),
        def!("abort-on-property-write.js", ["aopw.js"], false),
        def!("abort-current-script.js", ["acs.js", "abort-current-inline-script.js", "acis.js"], false),
        def!("no-setTimeout-if.js", ["nostif.js", "prevent-setTimeout.js", "setTimeout-defuser.js"], false),
        def!("no-setInterval-if.js", ["nosiif.js", "prevent-setInterval.js", "setInterval-defuser.js"], false),
        def!("adjust-setTimeout.js", ["nano-stb.js", "nano-setTimeout-booster.js"], false),
        def!("adjust-setInterval.js", ["nano-sib.js", "nano-setInterval-booster.js"], false),
        def!("prevent-addEventListener.js", ["aeld.js", "addEventListener-defuser.js"], false),
        def!("prevent-fetch.js", ["no-fetch-if.js"], false),
        def!("prevent-xhr.js", ["no-xhr-if.js"], false),
        def!("prevent-window-open.js", ["nowoif.js", "no-window-open-if.js", "window.open-defuser.js"], false),
        def!("remove-attr.js", ["ra.js"], false),
        def!("remove-class.js", ["rc.js"], false),
        def!("remove-node-text.js", ["rmnt.js"], false),
        def!("noeval-if.js", ["prevent-eval-if.js"], false),
        def!("href-sanitizer.js", [], false),
        def!("set-local-storage-item.js", [], false),
        def!("set-session-storage-item.js", [], false),
        def!("remove-cookie.js", ["cookie-remover.js"], false),
        def!("set-cookie.js", [], false),
        def!("nowebrtc.js", [], false),
        def!("abort-on-stack-trace.js", ["aost.js"], false),
        def!("adblock-detector-defuser.js", ["nobab.js", "bab-defuser.js", "nofab.js", "fuckadblock.js-3.2.0"], false),
    ]
}

/// Function resources must start with `function name(`: drop leading comments.
fn strip_header(src: &str) -> &str {
    match src.find("\nfunction ") {
        Some(i) if !src.starts_with("function ") => &src[i + 1..],
        _ => src,
    }
}

/// Scriptlets are `application/javascript` resources written as a named
/// function (called with the filter's arguments); shared helpers they
/// depend on are `fn/javascript` resources.
fn js_resource(name: &str, aliases: Vec<String>, deps: Vec<String>, permission: PermissionMask, src: &str, mime: MimeType) -> Resource {
    Resource {
        name: name.to_string(),
        aliases,
        kind: ResourceType::Mime(mime),
        content: STANDARD.encode(strip_header(src)),
        dependencies: deps,
        permission,
    }
}

/// All scriptlet resources, ready for `Engine::use_resources`.
pub fn resources() -> Vec<Resource> {
    let mut out = vec![js_resource("nf-utils.fn", vec![], vec![], PermissionMask::default(), UTILS, MimeType::FnJavascript)];
    for d in defs() {
        let perm = if d.trusted { TRUSTED } else { PermissionMask::default() };
        out.push(js_resource(
            d.name,
            d.aliases.iter().map(|a| a.to_string()).collect(),
            vec!["nf-utils.fn".to_string()],
            perm,
            d.source,
            MimeType::ApplicationJavascript,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scriptlet_starts_with_a_function() {
        for d in defs() {
            assert!(strip_header(d.source).starts_with("function "), "{}", d.name);
        }
        assert!(strip_header(UTILS).starts_with("function "));
    }
}
