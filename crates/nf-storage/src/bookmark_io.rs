//! Bookmarks in the « Netscape » HTML format that Chrome, Brave, Edge,
//! Firefox and Safari all export and import.
//!
//! Reading is tolerant (tags in any case, missing closing tags) but bounded:
//! only http(s) addresses are kept, folders nest at most `MAX_DEPTH` deep
//! and at most `MAX_ITEMS` entries are read.

use crate::{Bookmark, Folder, Result, Storage};

pub const MAX_DEPTH: usize = 32;
pub const MAX_ITEMS: usize = 50_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportNode {
    Folder { title: String, children: Vec<ImportNode> },
    Link { title: String, url: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportStats {
    pub added: usize,
    pub folders: usize,
    /// Already present in the same folder, or not an http(s) address.
    pub skipped: usize,
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|&e| e <= 10);
        let Some(end) = end else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => match ent.strip_prefix('#') {
                Some(n) => match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok().and_then(char::from_u32),
                    None => n.parse::<u32>().ok().and_then(char::from_u32),
                },
                None => None,
            },
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn encode(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Value of an attribute in the text of an opening tag (`<A HREF="…" …`).
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let after = lower[at + name.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let eq = at + name.len() + (lower[at + name.len()..].len() - after.len());
            let v = tag[eq + 1..].trim_start();
            let (q, body) = match v.chars().next() {
                Some(c @ ('"' | '\'')) => (Some(c), &v[1..]),
                _ => (None, v),
            };
            let end = match q {
                Some(c) => body.find(c).unwrap_or(body.len()),
                None => body.find(|c: char| c.is_ascii_whitespace() || c == '>').unwrap_or(body.len()),
            };
            return Some(decode_entities(&body[..end]));
        }
        from = at + name.len();
    }
    None
}

fn is_web(url: &str) -> bool {
    let l = url.trim().to_ascii_lowercase();
    (l.starts_with("https://") || l.starts_with("http://")) && url::Url::parse(url.trim()).is_ok()
}

/// Reads a bookmarks file. Entries that are not http(s) are left out.
pub fn parse_netscape(html: &str) -> Vec<ImportNode> {
    // Stack of open folders; the bottom one is the file itself.
    let mut stack: Vec<(String, Vec<ImportNode>)> = vec![(String::new(), Vec::new())];
    let mut pending_folder: Option<String> = None;
    let mut count = 0usize;
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        let closing = tag.starts_with('/');
        let after = &rest[gt + 1..];
        match (closing, name.as_str()) {
            (false, "h3") => {
                let end = after.to_ascii_lowercase().find("</h3").unwrap_or(after.len());
                pending_folder = Some(decode_entities(after[..end].trim()));
                rest = &after[end..];
                continue;
            }
            (false, "dl") => {
                if stack.len() <= MAX_DEPTH {
                    stack.push((pending_folder.take().unwrap_or_default(), Vec::new()));
                } else {
                    pending_folder = None;
                }
            }
            (true, "dl") => {
                if stack.len() > 1 {
                    let (title, children) = stack.pop().expect("len > 1");
                    // The outer <DL> of the file has no title: its entries go up.
                    if title.is_empty() && stack.len() == 1 {
                        stack[0].1.extend(children);
                    } else {
                        let title = if title.is_empty() { "Dossier".into() } else { title };
                        stack.last_mut().expect("bottom").1.push(ImportNode::Folder { title, children });
                    }
                }
            }
            (false, "a") => {
                let end = after.to_ascii_lowercase().find("</a").unwrap_or(after.len());
                let title = decode_entities(after[..end].trim());
                if let Some(href) = attr(tag, "href") {
                    if is_web(&href) && count < MAX_ITEMS {
                        count += 1;
                        let title = if title.is_empty() { href.trim().to_string() } else { title };
                        stack.last_mut().expect("bottom").1.push(ImportNode::Link { title: title.chars().take(500).collect(), url: href.trim().to_string() });
                    }
                }
                rest = &after[end..];
                continue;
            }
            _ => {}
        }
        rest = after;
    }
    // Unclosed folders: keep what was read.
    while stack.len() > 1 {
        let (title, children) = stack.pop().expect("len > 1");
        if title.is_empty() && stack.len() == 1 {
            stack[0].1.extend(children);
        } else {
            stack.last_mut().expect("bottom").1.push(ImportNode::Folder { title: if title.is_empty() { "Dossier".into() } else { title }, children });
        }
    }
    stack.pop().map(|(_, c)| c).unwrap_or_default()
}

/// Writes the bookmarks in the Netscape format (readable by every browser).
pub fn export_netscape(folders: &[Folder], bookmarks: &[Bookmark]) -> String {
    fn level(out: &mut String, parent: Option<i64>, depth: usize, folders: &[Folder], bookmarks: &[Bookmark]) {
        let pad = "    ".repeat(depth);
        for f in folders.iter().filter(|f| f.parent_id == parent) {
            out.push_str(&format!("{pad}<DT><H3>{}</H3>\n{pad}<DL><p>\n", encode(&f.title)));
            if depth < MAX_DEPTH {
                level(out, Some(f.id), depth + 1, folders, bookmarks);
            }
            out.push_str(&format!("{pad}</DL><p>\n"));
        }
        for b in bookmarks.iter().filter(|b| b.folder_id == parent) {
            out.push_str(&format!("{pad}<DT><A HREF=\"{}\" ADD_DATE=\"{}\">{}</A>\n", encode(&b.url), b.created.max(0), encode(&b.title)));
        }
    }
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Favoris NIGHTFALL</H1>\n<DL><p>\n",
    );
    level(&mut out, None, 1, folders, bookmarks);
    out.push_str("</DL><p>\n");
    out
}

impl Storage {
    /// Adds the nodes into a new folder `root_title`. A link already
    /// present with the same address in the same imported folder is not
    /// added twice; nothing existing is changed.
    pub fn import_bookmarks(&self, nodes: &[ImportNode], root_title: &str, now: i64) -> Result<ImportStats> {
        let mut stats = ImportStats::default();
        let root = self.add_folder(None, root_title, now)?;
        stats.folders += 1;
        self.import_level(nodes, root, now, &mut stats)?;
        Ok(stats)
    }

    fn import_level(&self, nodes: &[ImportNode], folder: i64, now: i64, stats: &mut ImportStats) -> Result<()> {
        let mut seen: Vec<&str> = Vec::new();
        for n in nodes {
            match n {
                ImportNode::Folder { title, children } => {
                    let id = self.add_folder(Some(folder), title, now)?;
                    stats.folders += 1;
                    self.import_level(children, id, now, stats)?;
                }
                ImportNode::Link { title, url } => {
                    if seen.contains(&url.as_str()) {
                        stats.skipped += 1;
                        continue;
                    }
                    seen.push(url);
                    self.add_bookmark(Some(folder), title, url, now)?;
                    stats.added += 1;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME: &str = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<!-- This is an automatically generated file. -->
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks</H1>
<DL><p>
    <DT><H3 ADD_DATE="1700000000" PERSONAL_TOOLBAR_FOLDER="true">Barre de favoris</H3>
    <DL><p>
        <DT><A HREF="https://fr.wikipedia.org/" ADD_DATE="1700000001" ICON="data:image/png;base64,AAA">Wikip&eacute;dia &amp; co</A>
        <DT><H3>Rust</H3>
        <DL><p>
            <DT><A HREF="https://doc.rust-lang.org/book/">The Book</A>
            <DT><A HREF='javascript:alert(1)'>Script</A>
            <DT><A HREF="https://doc.rust-lang.org/book/">The Book (copie)</A>
        </DL><p>
    </DL><p>
    <DT><a href=https://example.com/a?b=1&amp;c=2>Exemple</a>
    <DT><A HREF="place:sort=8">Récents</A>
</DL><p>
"#;

    #[test]
    fn chrome_file_is_read_with_nested_folders() {
        let nodes = parse_netscape(CHROME);
        assert_eq!(nodes.len(), 2);
        let ImportNode::Folder { title, children } = &nodes[0] else { panic!("dossier attendu") };
        assert_eq!(title, "Barre de favoris");
        assert_eq!(children[0], ImportNode::Link { title: "Wikip&eacute;dia & co".into(), url: "https://fr.wikipedia.org/".into() });
        let ImportNode::Folder { title, children: rust } = &children[1] else { panic!("sous-dossier attendu") };
        assert_eq!(title, "Rust");
        // javascript: left out; the duplicate is kept here and skipped on import.
        assert_eq!(rust.len(), 2);
        assert_eq!(nodes[1], ImportNode::Link { title: "Exemple".into(), url: "https://example.com/a?b=1&c=2".into() });
    }

    #[test]
    fn garbage_and_unclosed_files_do_not_panic() {
        assert!(parse_netscape("").is_empty());
        assert!(parse_netscape("<<<>>>&&&;<A HREF").is_empty());
        let n = parse_netscape("<DL><DT><H3>Ouvert</H3><DL><DT><A HREF=\"https://a.example/\">A</A>");
        assert_eq!(n.len(), 1);
        let deep = "<DL>".repeat(200) + "<A HREF=\"https://deep.example/\">x</A>";
        assert_eq!(count_links(&parse_netscape(&deep)), 1);
    }

    fn count_links(nodes: &[ImportNode]) -> usize {
        nodes.iter().map(|n| match n {
            ImportNode::Link { .. } => 1,
            ImportNode::Folder { children, .. } => count_links(children),
        }).sum()
    }

    #[test]
    fn export_then_import_round_trip() {
        let st = Storage::open_in_memory(&nf_crypto::SecretKey::from_bytes([42u8; 32])).unwrap();
        let f = st.add_folder(None, "Travail <&>", 1).unwrap();
        st.add_bookmark(Some(f), "Doc \"Rust\"", "https://doc.rust-lang.org/?a=1&b=2", 1).unwrap();
        st.add_bookmark(None, "Racine", "https://example.org/", 1).unwrap();
        let html = export_netscape(&st.folders().unwrap(), &st.bookmarks().unwrap());
        let nodes = parse_netscape(&html);
        assert_eq!(nodes.len(), 2);
        let ImportNode::Folder { title, children } = &nodes[0] else { panic!() };
        assert_eq!(title, "Travail <&>");
        assert_eq!(children[0], ImportNode::Link { title: "Doc \"Rust\"".into(), url: "https://doc.rust-lang.org/?a=1&b=2".into() });

        let st2 = Storage::open_in_memory(&nf_crypto::SecretKey::from_bytes([42u8; 32])).unwrap();
        let stats = st2.import_bookmarks(&parse_netscape(CHROME), "Importés", 5).unwrap();
        assert_eq!(stats, ImportStats { added: 3, folders: 3, skipped: 1 });
        assert_eq!(st2.bookmarks().unwrap().len(), 3);
        assert_eq!(st2.folders().unwrap().iter().filter(|f| f.title == "Rust").count(), 1);
    }
}
