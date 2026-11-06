//! Themes: a few base colours chosen by the user (or a preset), from which
//! NIGHTFALL derives every colour of the interface, then serves them as
//! `nightfall://app/theme.css` (custom properties read by every page and by
//! the tab strip). Contrast is enforced: all text ≥ 4.5:1 on its surfaces,
//! accents ≥ 3:1 on the background (WCAG 2.2).

use serde::{Deserialize, Serialize};

/// The colours a theme chooses (sRGB `#rrggbb`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub id: String,
    pub name: String,
    /// Pages and tab strip.
    pub bg: String,
    /// Toolbar, menus, panels.
    pub surface: String,
    pub text: String,
    /// Primary actions, selection.
    pub accent: String,
    /// Focus, active tab line.
    pub accent2: String,
}

impl Default for Theme {
    fn default() -> Self {
        presets().into_iter().next().expect("presets")
    }
}

/// Saved in the encrypted settings. Custom themes are kept as raw JSON and
/// read one by one: an invalid or unknown entry is ignored, never a reason
/// to lose the other settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    /// Id of a preset (`crepuscule`…) or of a custom theme (`custom-…`).
    pub active: String,
    pub custom: Vec<serde_json::Value>,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self { active: "crepuscule".into(), custom: Vec::new() }
    }
}

fn t(id: &str, name: &str, bg: &str, surface: &str, text: &str, accent: &str, accent2: &str) -> Theme {
    Theme { id: id.into(), name: name.into(), bg: bg.into(), surface: surface.into(), text: text.into(), accent: accent.into(), accent2: accent2.into() }
}

/// Built-in themes (the first one is the default).
pub fn presets() -> Vec<Theme> {
    vec![
        t("crepuscule", "Crépuscule", "#05070c", "#0e121d", "#eef2fb", "#2967ff", "#5ad2e8"),
        t("minuit", "Minuit", "#000000", "#0a0a0f", "#f2f2f7", "#3867ff", "#7aa2ff"),
        t("aurore", "Aurore", "#061210", "#0c1b18", "#e9f7f2", "#0a7355", "#3fe0b0"),
        t("braise", "Braise", "#110807", "#1b0f0d", "#fbeee9", "#c4472a", "#ff9a5c"),
        t("brume", "Brume", "#0b0d12", "#151922", "#e6e9f0", "#4f5f9e", "#9fb0e8"),
        t("aube", "Aube (clair)", "#f6f7fb", "#ffffff", "#141824", "#2253e0", "#0b7a99"),
    ]
}

pub fn custom_themes(s: &ThemeSettings) -> Vec<Theme> {
    s.custom
        .iter()
        .filter_map(|v| serde_json::from_value::<Theme>(v.clone()).ok())
        .filter(|t| t.id.starts_with("custom-") && validate(t).is_ok())
        .collect()
}

/// Theme in use: the chosen one, or the default if it no longer exists.
pub fn active_theme(s: &ThemeSettings) -> Theme {
    presets()
        .into_iter()
        .chain(custom_themes(s))
        .find(|t| t.id == s.active)
        .unwrap_or_default()
}

fn parse_hex(c: &str) -> Option<[u8; 3]> {
    let h = c.strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn lin(v: u8) -> f64 {
    let c = v as f64 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn luminance(c: [u8; 3]) -> f64 {
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG contrast ratio.
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `a` mixed with `b` (`w` = share of `a`), in sRGB.
fn mix(a: [u8; 3], b: [u8; 3], w: f64) -> [u8; 3] {
    let m = |x: u8, y: u8| (x as f64 * w + y as f64 * (1.0 - w)).round().clamp(0.0, 255.0) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

/// The most muted mix of `text` towards `bg` that still reaches `min`
/// contrast on every surface of `on`.
fn muted(text: [u8; 3], bg: [u8; 3], on: &[[u8; 3]], start: f64, min: f64) -> [u8; 3] {
    let mut w = start;
    while w < 1.0 {
        let c = mix(text, bg, w);
        if on.iter().all(|s| contrast(c, *s) >= min) {
            return c;
        }
        w += 0.02;
    }
    text
}

/// Why a theme cannot be used (French, shown to the user).
pub fn validate(t: &Theme) -> Result<(), String> {
    let get = |name: &str, v: &str| parse_hex(v).ok_or_else(|| format!("couleur « {name} » invalide (format #rrggbb)"));
    let bg = get("fond", &t.bg)?;
    let surface = get("surfaces", &t.surface)?;
    let text = get("texte", &t.text)?;
    let accent = get("accent", &t.accent)?;
    let accent2 = get("second accent", &t.accent2)?;
    if t.name.trim().is_empty() || t.name.chars().count() > 40 {
        return Err("nom du thème vide ou trop long (40 caractères au plus)".into());
    }
    for (what, s) in [("le fond", bg), ("les surfaces", surface)] {
        let r = contrast(text, s);
        if r < 4.5 {
            return Err(format!("texte trop peu lisible sur {what} (contraste {r:.1}:1, il faut au moins 4,5:1)"));
        }
    }
    for (what, a) in [("l'accent", accent), ("le second accent", accent2)] {
        let r = contrast(a, bg);
        if r < 3.0 {
            return Err(format!("{what} se distingue trop peu du fond (contraste {r:.1}:1, il faut au moins 3:1)"));
        }
    }
    let on = on_accent(accent);
    if contrast(on, accent) < 4.5 {
        return Err("aucune couleur de texte n'est lisible sur cet accent : choisissez un accent plus clair ou plus foncé".into());
    }
    Ok(())
}

/// White or near-black, whichever reads best on the accent.
fn on_accent(accent: [u8; 3]) -> [u8; 3] {
    let white = [255, 255, 255];
    let ink = [10, 12, 18];
    if contrast(white, accent) >= contrast(ink, accent) { white } else { ink }
}

fn is_light(bg: [u8; 3]) -> bool {
    luminance(bg) > 0.4
}

/// Custom properties of a theme (every colour token of the interface).
/// Added to theme.css when the user asks for fewer animations: every
/// animation and transition of NIGHTFALL's pages ends at once.
pub const REDUCED_MOTION_CSS: &str = "*,*::before,*::after{animation-duration:0s!important;animation-delay:0s!important;transition-duration:0s!important;transition-delay:0s!important;scroll-behavior:auto!important}\n";

pub fn css(t: &Theme) -> String {
    let t = if validate(t).is_ok() { t.clone() } else { Theme::default() };
    let p = |c: &str| parse_hex(c).expect("validated");
    let (bg, surface, text, accent, accent2) = (p(&t.bg), p(&t.surface), p(&t.text), p(&t.accent), p(&t.accent2));
    let light = is_light(bg);
    let raised = mix(text, surface, if light { 0.04 } else { 0.05 });
    let raised = if light { mix(surface, bg, 0.6) } else { raised };
    let overlay = mix(text, surface, if light { 0.08 } else { 0.10 });
    let surfaces = [bg, surface, raised];
    let text2 = muted(text, bg, &surfaces, 0.55, 4.6);
    let text3 = muted(text, bg, &surfaces, 0.50, 4.5);
    let hover = if light { mix(accent, [255, 255, 255], 0.85) } else { mix(accent, [255, 255, 255], 0.86) };
    let accent_hover = if light { mix(accent, [0, 0, 0], 0.88) } else { hover };
    format!(
        ":root:root{{--c-bg:{bg};--c-surface:{surface};--c-raised:{raised};--c-overlay:{overlay};--c-text:{text};--c-text-2:{text2};--c-text-3:{text3};--c-accent:{accent};--c-accent-hover:{accent_hover};--c-accent-2:{accent2};--c-on-accent:{on};color-scheme:{scheme};}}\n",
        bg = hex(bg),
        surface = hex(surface),
        raised = hex(raised),
        overlay = hex(overlay),
        text = hex(text),
        text2 = hex(text2),
        text3 = hex(text3),
        accent = hex(accent),
        accent_hover = hex(accent_hover),
        accent2 = hex(accent2),
        on = hex(on_accent(accent)),
        scheme = if light { "light" } else { "dark" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_valid_and_readable() {
        for p in presets() {
            assert!(validate(&p).is_ok(), "{} : {:?}", p.id, validate(&p));
            let css = css(&p);
            // Derived secondary texts stay readable on every surface.
            let var = |name: &str| {
                let i = css.find(&format!("--c-{name}:")).unwrap() + name.len() + 5;
                parse_hex(&css[i..i + 7]).unwrap()
            };
            for surf in ["bg", "surface", "raised"] {
                for txt in ["text", "text-2", "text-3"] {
                    let r = contrast(var(txt), var(surf));
                    assert!(r >= 4.5, "{} : {txt} sur {surf} = {r:.2}", p.id);
                }
            }
            assert!(contrast(var("on-accent"), var("accent")) >= 4.5, "{} : texte sur l'accent", p.id);
        }
    }

    #[test]
    fn unreadable_or_malformed_themes_are_refused() {
        let mut t = Theme::default();
        t.text = "#101010".into();
        assert!(validate(&t).unwrap_err().contains("texte"));
        let mut t = Theme::default();
        t.accent = "red; } body { display:none".into();
        assert!(validate(&t).unwrap_err().contains("invalide"));
        // Never any free text in the generated CSS.
        assert!(!css(&t).contains("display"));
    }

    #[test]
    fn saved_settings_are_read_tolerantly() {
        let s: ThemeSettings = serde_json::from_str(r##"{"active":"custom-1","custom":[
            {"id":"custom-1","name":"Mien","bg":"#000000","surface":"#111111","text":"#ffffff","accent":"#3867ff","accent2":"#7aa2ff"},
            {"id":"custom-2","name":"Cassé","bg":"nope"},
            {"weird":true},
            "pas un objet"
        ],"future_field":1}"##).unwrap();
        let customs = custom_themes(&s);
        assert_eq!(customs.len(), 1);
        assert_eq!(active_theme(&s).name, "Mien");
        let gone = ThemeSettings { active: "custom-9".into(), ..s };
        assert_eq!(active_theme(&gone).id, "crepuscule");
        let empty: ThemeSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(active_theme(&empty).id, "crepuscule");
    }
}
