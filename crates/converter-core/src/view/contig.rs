//! Shared contig-name aliases (chr1 ↔ 1, chrM ↔ MT, …).

/// Common chromosome aliases for a requested contig name.
pub fn contig_name_aliases(name: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(6);
    out.push(name.to_string());
    let lower = name.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("chr") {
        if !rest.is_empty() {
            let bare = name.get(3..).unwrap_or(rest).to_string();
            out.push(bare);
            if rest == "m" || rest == "mt" {
                out.push("MT".to_string());
                out.push("M".to_string());
                out.push("chrM".to_string());
                out.push("chrMT".to_string());
            }
        }
    } else {
        out.push(format!("chr{name}"));
        if lower == "mt" || lower == "m" {
            out.push("chrM".to_string());
            out.push("chrMT".to_string());
            out.push("MT".to_string());
            out.push("M".to_string());
        }
    }
    out
}

/// Resolve `requested` against a list of contig names (exact, alias, then case-insensitive).
pub fn resolve_contig_name<'a>(names: impl IntoIterator<Item = &'a str>, requested: &str) -> Option<String> {
    let names: Vec<&'a str> = names.into_iter().collect();
    for alias in contig_name_aliases(requested) {
        if let Some(hit) = names.iter().find(|n| **n == alias) {
            return Some((*hit).to_string());
        }
    }
    names
        .iter()
        .find(|n| n.eq_ignore_ascii_case(requested))
        .map(|n| (*n).to_string())
}

/// Like [`resolve_contig_name`] but returns a reference into `items` via a name getter.
pub fn find_named<'a, T>(items: &'a [T], requested: &str, name_of: impl Fn(&T) -> &str) -> Option<&'a T> {
    for alias in contig_name_aliases(requested) {
        if let Some(hit) = items.iter().find(|c| name_of(c) == alias) {
            return Some(hit);
        }
    }
    items
        .iter()
        .find(|c| name_of(c).eq_ignore_ascii_case(requested))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_chr_and_bare() {
        let names = ["chr1", "chrM"];
        assert_eq!(resolve_contig_name(names, "1").as_deref(), Some("chr1"));
        assert_eq!(resolve_contig_name(names, "chr1").as_deref(), Some("chr1"));
        assert_eq!(resolve_contig_name(names, "MT").as_deref(), Some("chrM"));
        assert_eq!(resolve_contig_name(["1"], "chr1").as_deref(), Some("1"));
    }
}
