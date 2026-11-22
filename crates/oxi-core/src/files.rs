//! Operações de gerenciador de arquivos. Excluir sempre envia para a lixeira.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Oculto? No Windows, usa os atributos oculto/sistema (nomes com ponto são visíveis);
/// nos demais sistemas, nomes começando com ponto.
#[cfg(windows)]
fn is_hidden(_name: &str, meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _meta: &fs::Metadata) -> bool {
    name.starts_with('.')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Modified,
}

/// Lista um diretório. Pastas primeiro; entradas ilegíveis são ignoradas.
pub fn list_dir(dir: &Path, show_hidden: bool, sort: SortKey) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for item in fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let Ok(meta) = item.metadata() else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if !show_hidden && is_hidden(&name, &meta) {
            continue;
        }
        out.push(Entry {
            name,
            path: item.path(),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta.modified().ok(),
        });
    }
    out.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| match sort {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Size => b.size.cmp(&a.size),
            SortKey::Modified => b.modified.cmp(&a.modified),
        })
    });
    Ok(out)
}

/// Cria um nome livre ("copia", "copia (2)", ...) quando o destino já existe.
fn free_name(dest: &Path) -> PathBuf {
    if !dest.exists() {
        return dest.to_path_buf();
    }
    let stem = dest.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = dest.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let parent = dest.parent().unwrap_or(Path::new("."));
    let mut n = 1;
    loop {
        let label = if n == 1 { " - copia".to_string() } else { format!(" - copia ({n})") };
        let cand = parent.join(format!("{stem}{label}{ext}"));
        if !cand.exists() {
            return cand;
        }
        n += 1;
    }
}

/// Copia arquivo ou pasta (recursivo). Nunca sobrescreve: usa nome livre.
/// Não segue links simbólicos. Recusa copiar uma pasta para dentro dela mesma.
pub fn copy_any(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let name = src.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "origem sem nome"))?;
    let target = free_name(&dest_dir.join(name));
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "links simbólicos não são copiados"));
    }
    if meta.is_dir() {
        let src_c = fs::canonicalize(src)?;
        if fs::canonicalize(dest_dir)?.starts_with(&src_c) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "destino está dentro da origem"));
        }
        fs::create_dir_all(&target)?;
        for item in fs::read_dir(src)? {
            copy_any(&item?.path(), &target)?;
        }
    } else {
        fs::copy(src, &target)?;
    }
    Ok(target)
}

/// Move arquivo ou pasta; tenta `rename` e cai para copiar+lixeira entre volumes.
pub fn move_any(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let name = src.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "origem sem nome"))?;
    let target = free_name(&dest_dir.join(name));
    match fs::rename(src, &target) {
        Ok(()) => Ok(target),
        Err(_) => {
            let copied = copy_any(src, dest_dir)?;
            send_to_trash(src)?;
            Ok(copied)
        }
    }
}

pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    if new_name.is_empty() || new_name.contains(['/', '\\']) || new_name == "." || new_name == ".." {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "nome inválido"));
    }
    let target = path.with_file_name(new_name);
    if target.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "já existe um item com esse nome"));
    }
    fs::rename(path, &target)?;
    Ok(target)
}

pub fn make_dir(parent: &Path, name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "nome inválido"));
    }
    let p = parent.join(name);
    fs::create_dir(&p)?;
    Ok(p)
}

pub fn send_to_trash(path: &Path) -> io::Result<()> {
    trash::delete(path).map_err(|e| io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_dirs_first_and_hides_dotfiles() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("b.txt"), "x").unwrap();
        fs::write(d.path().join(".hidden"), "x").unwrap();
        fs::create_dir(d.path().join("zdir")).unwrap();
        let v = list_dir(d.path(), false, SortKey::Name).unwrap();
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["zdir", "b.txt"]);
        assert_eq!(list_dir(d.path(), true, SortKey::Name).unwrap().len(), 3);
    }

    #[test]
    fn copy_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("a.txt"), "1").unwrap();
        let out = copy_any(&d.path().join("a.txt"), d.path()).unwrap();
        assert_eq!(out.file_name().unwrap(), "a - copia.txt");
        let out2 = copy_any(&d.path().join("a.txt"), d.path()).unwrap();
        assert_eq!(out2.file_name().unwrap(), "a - copia (2).txt");
        assert_eq!(fs::read_to_string(d.path().join("a.txt")).unwrap(), "1");
    }

    #[test]
    fn copy_dir_recursive_and_refuses_into_self() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("sub/f.txt"), "hi").unwrap();
        let dst = d.path().join("dst");
        fs::create_dir(&dst).unwrap();
        let out = copy_any(&src, &dst).unwrap();
        assert_eq!(fs::read_to_string(out.join("sub/f.txt")).unwrap(), "hi");
        assert!(copy_any(&src, &src.join("sub")).is_err());
    }

    #[test]
    fn rename_validates() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.txt");
        fs::write(&f, "1").unwrap();
        fs::write(d.path().join("b.txt"), "2").unwrap();
        assert!(rename(&f, "").is_err());
        assert!(rename(&f, "x/y").is_err());
        assert!(rename(&f, "b.txt").is_err());
        let n = rename(&f, "c.txt").unwrap();
        assert!(n.exists() && !f.exists());
    }

    #[test]
    fn move_works() {
        let d = tempfile::tempdir().unwrap();
        let dst = d.path().join("dst");
        fs::create_dir(&dst).unwrap();
        fs::write(d.path().join("a.txt"), "1").unwrap();
        let out = move_any(&d.path().join("a.txt"), &dst).unwrap();
        assert!(out.exists() && !d.path().join("a.txt").exists());
    }
}
