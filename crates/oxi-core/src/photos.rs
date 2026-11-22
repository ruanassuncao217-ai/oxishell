//! Navegação em pastas de imagens (a decodificação fica na interface).

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

pub const EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff"];

pub fn is_image(path: &Path) -> bool {
    path.extension().map(|e| EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str())).unwrap_or(false)
}

/// Comparação "natural": foto2 < foto10.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                let mut nb = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (na, nb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

pub fn images_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_image(p)).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| natural_cmp(&a.file_name().unwrap_or_default().to_string_lossy(), &b.file_name().unwrap_or_default().to_string_lossy()));
    v
}

pub fn step(len: usize, current: usize, forward: bool) -> usize {
    if len == 0 {
        0
    } else if forward {
        (current + 1) % len
    } else {
        (current + len - 1) % len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("foto2.jpg", "foto10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a01"), Ordering::Equal);
        assert_eq!(natural_cmp("b", "a9"), Ordering::Greater);
    }

    #[test]
    fn lists_only_images_in_natural_order() {
        let d = tempfile::tempdir().unwrap();
        for n in ["f10.PNG", "f2.jpg", "notes.txt", "f1.gif"] {
            fs::write(d.path().join(n), "x").unwrap();
        }
        let names: Vec<_> = images_in(d.path()).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["f1.gif", "f2.jpg", "f10.PNG"]);
    }

    #[test]
    fn stepping_wraps() {
        assert_eq!(step(3, 2, true), 0);
        assert_eq!(step(3, 0, false), 2);
        assert_eq!(step(0, 0, true), 0);
    }
}
