//! Passwords as CSV, the format Chrome, Brave, Edge, Firefox, Safari and
//! Bitwarden export. Columns are found by their header name, so the order
//! of each browser does not matter.
//!
//! A file whose header has no address or no password column, or with a
//! malformed row, is refused as a whole: nothing is imported partially.

use crate::{normalize_origin, Vault, VaultError};
use zeroize::Zeroize;

pub const MAX_ROWS: usize = 20_000;

/// One row read from the file.
#[derive(Clone, PartialEq, Eq)]
pub struct CsvEntry {
    pub origin: String,
    pub username: String,
    pub password: String,
    pub notes: String,
}

impl std::fmt::Debug for CsvEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsvEntry").field("origin", &self.origin).field("username", &self.username).finish_non_exhaustive()
    }
}

impl Drop for CsvEntry {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct CsvImportStats {
    pub added: usize,
    /// Same address, user name and password already saved.
    pub duplicates: usize,
    /// Rows whose address is not http(s) (apps, android://…).
    pub skipped: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CsvError {
    #[error("ce fichier n'a pas de colonne d'adresse et de mot de passe reconnue")]
    Header,
    #[error("ligne {0} mal formée")]
    Row(usize),
    #[error("fichier trop grand (plus de 20 000 lignes)")]
    TooLarge,
}

/// RFC 4180 records: quoted fields with "" for a quote, CRLF or LF.
fn records(text: &str) -> Result<Vec<Vec<String>>, CsvError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut at_start = true;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => {
                quoted = true;
                at_start = false;
            }
            '"' => return Err(CsvError::Row(out.len() + 1)),
            ',' => {
                row.push(std::mem::take(&mut field));
                at_start = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty()) {
                    out.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
                at_start = true;
                if out.len() > MAX_ROWS + 1 {
                    return Err(CsvError::TooLarge);
                }
            }
            _ => {
                field.push(c);
                at_start = false;
            }
        }
    }
    if quoted {
        return Err(CsvError::Row(out.len() + 1));
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        out.push(row);
    }
    Ok(out)
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header.iter().position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
}

/// Reads a password export. Rows for non-web addresses are counted in
/// `skipped`; any malformed row refuses the whole file.
pub fn parse_csv(text: &str) -> Result<(Vec<CsvEntry>, usize), CsvError> {
    let recs = records(text)?;
    let Some((header, rows)) = recs.split_first() else { return Err(CsvError::Header) };
    let url = column(header, &["url", "login_uri", "website", "origin", "web site"]).ok_or(CsvError::Header)?;
    let pass = column(header, &["password", "login_password"]).ok_or(CsvError::Header)?;
    let user = column(header, &["username", "login_username", "user name", "login"]);
    let note = column(header, &["note", "notes", "comment", "extra"]);
    if rows.len() > MAX_ROWS {
        return Err(CsvError::TooLarge);
    }
    let mut entries = Vec::new();
    let mut skipped = 0;
    for (i, r) in rows.iter().enumerate() {
        if r.len() != header.len() {
            return Err(CsvError::Row(i + 2));
        }
        match normalize_origin(r[url].trim()) {
            Ok(origin) => entries.push(CsvEntry {
                origin,
                username: user.map(|u| r[u].clone()).unwrap_or_default(),
                password: r[pass].clone(),
                notes: note.map(|n| r[n].clone()).unwrap_or_default(),
            }),
            Err(_) => skipped += 1,
        }
    }
    Ok((entries, skipped))
}

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with([' ', '=', '+', '-', '@']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl Vault {
    /// Adds the entries; one already saved with the same origin, user name
    /// and password is not added again. Saved once, at the end.
    pub fn import_entries(&mut self, entries: &[CsvEntry], now: i64) -> Result<CsvImportStats, VaultError> {
        let mut stats = CsvImportStats::default();
        for e in entries {
            let dup = self.data.entries.iter().any(|c| c.origin == e.origin && c.username == e.username && c.password == e.password);
            if dup {
                stats.duplicates += 1;
                continue;
            }
            let id = nf_crypto::hex(&nf_crypto::random_bytes(12)?);
            self.data.entries.push(crate::Credential {
                id,
                origin: e.origin.clone(),
                username: e.username.clone(),
                password: e.password.clone(),
                notes: e.notes.clone(),
                created: now,
                updated: now,
            });
            stats.added += 1;
        }
        if stats.added > 0 {
            self.save()?;
        }
        Ok(stats)
    }

    /// Every entry in the CSV layout of Chrome (name,url,username,password,note).
    /// The text is in clear: the caller warns the user and asks for the code.
    pub fn export_csv(&self) -> String {
        let mut out = String::from("name,url,username,password,note\n");
        let mut entries: Vec<&crate::Credential> = self.data.entries.iter().collect();
        entries.sort_by(|a, b| a.origin.cmp(&b.origin).then(a.username.cmp(&b.username)));
        for c in entries {
            let name = url::Url::parse(&c.origin).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
            out.push_str(&format!("{},{},{},{},{}\n", quote(&name), quote(&c.origin), quote(&c.username), quote(&c.password), quote(&c.notes)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nf_crypto::SecretKey;

    #[test]
    fn chrome_firefox_and_safari_files_are_read() {
        let chrome = "name,url,username,password,note\r\nexample.com,https://example.com/login,alice,\"p,a\"\"ss\",\"ligne 1\nligne 2\"\r\nappli,android://abc@com.app/,bob,x,\r\n";
        let (e, skipped) = parse_csv(chrome).unwrap();
        assert_eq!(skipped, 1);
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].origin.as_str(), e[0].username.as_str(), e[0].password.as_str()), ("https://example.com", "alice", "p,a\"ss"));
        assert_eq!(e[0].notes, "ligne 1\nligne 2");

        let firefox = "\u{feff}\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\"https://a.example\",\"u\",\"pw\",,\"https://a.example\",\"{x}\",\"1\",\"1\",\"1\"\n";
        let (e, _) = parse_csv(firefox).unwrap();
        assert_eq!((e[0].origin.as_str(), e[0].password.as_str()), ("https://a.example", "pw"));

        let safari = "Title,URL,Username,Password,Notes,OTPAuth\nA,https://b.example/x,me,secret,,\n";
        assert_eq!(parse_csv(safari).unwrap().0[0].origin, "https://b.example");
    }

    #[test]
    fn malformed_files_are_refused_whole() {
        assert_eq!(parse_csv("").unwrap_err(), CsvError::Header);
        assert_eq!(parse_csv("nom,adresse\nx,y\n").unwrap_err(), CsvError::Header);
        assert_eq!(parse_csv("url,password\nhttps://a.example,1\nhttps://b.example\n").unwrap_err(), CsvError::Row(3));
        assert_eq!(parse_csv("url,password\n\"https://a.example,1\n").unwrap_err(), CsvError::Row(2));
        assert_eq!(parse_csv("url,password\nhttps://a.ex\"ample,1\n").unwrap_err(), CsvError::Row(2));
    }

    #[test]
    fn import_skips_duplicates_and_export_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path().join("vault.bin"), SecretKey::generate().unwrap()).unwrap();
        v.add("https://example.com", "alice", "déjà", "", 1).unwrap();
        let (e, _) = parse_csv("url,username,password\nhttps://example.com,alice,déjà\nhttps://example.com,alice,nouveau\nhttps://c.example,=cmd,-x\n").unwrap();
        let s = v.import_entries(&e, 2).unwrap();
        assert_eq!((s.added, s.duplicates), (2, 1));
        assert_eq!(v.len(), 3);
        let csv = v.export_csv();
        // Values that a spreadsheet would run as a formula are quoted.
        assert!(csv.contains(",\"=cmd\",\"-x\","));
        let (back, skipped) = parse_csv(&csv).unwrap();
        assert_eq!((back.len(), skipped), (3, 0));
        assert!(back.iter().any(|b| b.password == "nouveau"));
    }
}
