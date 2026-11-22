//! Mapa de uso de disco: varre uma pasta e agrega tamanhos.

use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Node>,
}

/// Varre `path` (sem seguir links simbólicos). `max_depth` limita apenas quantos
/// níveis são guardados em `children`; os tamanhos continuam somando tudo abaixo.
pub fn scan(path: &Path, max_depth: usize) -> io::Result<Node> {
    let meta = fs::symlink_metadata(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    if !meta.is_dir() {
        let size = if meta.file_type().is_symlink() { 0 } else { meta.len() };
        return Ok(Node { name, size, is_dir: false, children: Vec::new() });
    }
    let mut size = 0;
    let mut children = Vec::new();
    for item in fs::read_dir(path)? {
        let Ok(item) = item else { continue };
        let Ok(child) = scan(&item.path(), max_depth.saturating_sub(1)) else { continue };
        size += child.size;
        if max_depth > 0 {
            children.push(child);
        }
    }
    children.sort_by_key(|c| std::cmp::Reverse(c.size));
    Ok(Node { name, size, is_dir: true, children })
}

/// Os `n` maiores arquivos dentro da árvore (apenas níveis guardados).
pub fn largest_files(node: &Node, n: usize) -> Vec<(String, u64)> {
    fn walk(node: &Node, prefix: &str, out: &mut Vec<(String, u64)>) {
        let here = if prefix.is_empty() { node.name.clone() } else { format!("{prefix}/{}", node.name) };
        if node.is_dir {
            for c in &node.children {
                walk(c, &here, out);
            }
        } else {
            out.push((here, node.size));
        }
    }
    let mut v = Vec::new();
    walk(node, "", &mut v);
    v.sort_by_key(|f| std::cmp::Reverse(f.1));
    v.truncate(n);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_sizes_and_sorts() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("a")).unwrap();
        fs::write(d.path().join("a/big.bin"), vec![0u8; 1000]).unwrap();
        fs::write(d.path().join("small.bin"), vec![0u8; 10]).unwrap();
        let n = scan(d.path(), 5).unwrap();
        assert_eq!(n.size, 1010);
        assert_eq!(n.children[0].name, "a");
        assert_eq!(n.children[0].size, 1000);
        let top = largest_files(&n, 1);
        assert_eq!(top[0].1, 1000);
        assert!(top[0].0.ends_with("a/big.bin"));
    }

    #[test]
    fn depth_limit_keeps_total() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("a/b")).unwrap();
        fs::write(d.path().join("a/b/f"), vec![0u8; 50]).unwrap();
        let n = scan(d.path(), 1).unwrap();
        assert_eq!(n.size, 50);
        assert!(n.children[0].children.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("f"), vec![0u8; 100]).unwrap();
        std::os::unix::fs::symlink(d.path(), d.path().join("loop")).unwrap();
        assert_eq!(scan(d.path(), 3).unwrap().size, 100);
    }
}
