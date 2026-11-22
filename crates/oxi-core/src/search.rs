//! Índice de nomes de arquivo em memória com busca por substring, fuzzy e filtros.
//!
//! Sintaxe da consulta: termos separados por espaço (todos precisam casar),
//! `ext:rs` filtra por extensão, `dir:` mostra só pastas, `file:` só arquivos.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub path: PathBuf,
    pub name: String,
    pub name_lower: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Default)]
pub struct Index {
    pub entries: Vec<IndexEntry>,
}

impl Index {
    /// Indexa `roots` recursivamente (sem seguir links). Para ao atingir `limit` entradas.
    pub fn build(roots: &[PathBuf], limit: usize) -> Index {
        let mut idx = Index::default();
        let mut stack: Vec<PathBuf> = roots.to_vec();
        while let Some(dir) = stack.pop() {
            if idx.entries.len() >= limit {
                break;
            }
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for item in rd.flatten() {
                let Ok(meta) = fs::symlink_metadata(item.path()) else { continue };
                let name = item.file_name().to_string_lossy().into_owned();
                let is_dir = meta.is_dir();
                idx.entries.push(IndexEntry {
                    path: item.path(),
                    name_lower: name.to_lowercase(),
                    name,
                    is_dir,
                    size: if is_dir { 0 } else { meta.len() },
                });
                if is_dir {
                    stack.push(item.path());
                }
                if idx.entries.len() >= limit {
                    break;
                }
            }
        }
        idx
    }

    pub fn query(&self, q: &str, max_results: usize) -> Vec<&IndexEntry> {
        let parsed = Query::parse(q);
        if parsed.terms.is_empty() && parsed.ext.is_none() {
            return Vec::new();
        }
        let mut scored: Vec<(u32, &IndexEntry)> = Vec::new();
        for e in &self.entries {
            if let Some(want_dir) = parsed.only_dirs {
                if e.is_dir != want_dir {
                    continue;
                }
            }
            if let Some(ext) = &parsed.ext {
                let has = Path::new(&e.name_lower).extension().map(|x| x.to_string_lossy() == ext.as_str()).unwrap_or(false);
                if !has {
                    continue;
                }
            }
            let mut total = 0;
            let mut ok = true;
            for t in &parsed.terms {
                match score(&e.name_lower, t) {
                    Some(s) => total += s,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                scored.push((total, e));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.len().cmp(&b.1.name.len())));
        scored.into_iter().take(max_results).map(|(_, e)| e).collect()
    }
}

struct Query {
    terms: Vec<String>,
    ext: Option<String>,
    only_dirs: Option<bool>,
}

impl Query {
    fn parse(q: &str) -> Query {
        let mut terms = Vec::new();
        let mut ext = None;
        let mut only_dirs = None;
        for tok in q.split_whitespace() {
            let low = tok.to_lowercase();
            if let Some(e) = low.strip_prefix("ext:") {
                if !e.is_empty() {
                    ext = Some(e.trim_start_matches('.').to_string());
                }
            } else if low == "dir:" {
                only_dirs = Some(true);
            } else if low == "file:" {
                only_dirs = Some(false);
            } else {
                terms.push(low);
            }
        }
        Query { terms, ext, only_dirs }
    }
}

/// Pontuação: exato > prefixo > substring > subsequência (fuzzy). `None` = não casa.
fn score(name: &str, term: &str) -> Option<u32> {
    if name == term {
        return Some(1000);
    }
    if name.starts_with(term) {
        return Some(500);
    }
    if name.contains(term) {
        return Some(300);
    }
    let mut it = name.chars();
    let mut gaps = 0u32;
    for c in term.chars() {
        let mut found = false;
        for n in it.by_ref() {
            if n == c {
                found = true;
                break;
            }
            gaps += 1;
        }
        if !found {
            return None;
        }
    }
    Some(100u32.saturating_sub(gaps.min(90)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (tempfile::TempDir, Index) {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("report.pdf"), "x").unwrap();
        fs::write(d.path().join("reports_old.txt"), "x").unwrap();
        fs::write(d.path().join("main.rs"), "x").unwrap();
        fs::create_dir(d.path().join("reports")).unwrap();
        let idx = Index::build(&[d.path().to_path_buf()], 1000);
        (d, idx)
    }

    #[test]
    fn ranks_exact_prefix_substring() {
        let (_d, idx) = sample();
        let r = idx.query("report.pdf", 10);
        assert_eq!(r[0].name, "report.pdf");
        let r = idx.query("report", 10);
        assert!(r.iter().all(|e| e.name_lower.contains("report")));
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn fuzzy_and_filters() {
        let (_d, idx) = sample();
        assert_eq!(idx.query("mn", 10)[0].name, "main.rs");
        assert_eq!(idx.query("ext:rs", 10).len(), 1);
        let dirs = idx.query("report dir:", 10);
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].is_dir);
        assert!(idx.query("zzzz", 10).is_empty());
        assert!(idx.query("", 10).is_empty());
    }

    #[test]
    fn limit_is_respected() {
        let d = tempfile::tempdir().unwrap();
        for i in 0..20 {
            fs::write(d.path().join(format!("f{i}")), "x").unwrap();
        }
        let idx = Index::build(&[d.path().to_path_buf()], 5);
        assert_eq!(idx.entries.len(), 5);
    }
}
