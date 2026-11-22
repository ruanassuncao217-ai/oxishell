//! Limpeza de arquivos temporários. Começa sempre em modo simulação.
//!
//! Proteções: só remove arquivos (nunca pastas fora das raízes), ignora links
//! simbólicos, respeita idade mínima e recusa raízes rasas (ex.: `C:\`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct Category {
    pub id: &'static str,
    pub name: &'static str,
    pub roots: Vec<PathBuf>,
    pub min_age_days: u64,
}

/// Categorias padrão para o Windows (variáveis de ambiente que não existirem são ignoradas).
pub fn default_categories() -> Vec<Category> {
    let var = |k: &str| env::var_os(k).map(PathBuf::from);
    let mut temp_roots = Vec::new();
    if let Some(t) = var("TEMP") {
        temp_roots.push(t);
    }
    if let Some(l) = var("LOCALAPPDATA") {
        temp_roots.push(l.join("Temp"));
    }
    temp_roots.sort();
    temp_roots.dedup();
    let mut cats = vec![Category { id: "temp", name: "Arquivos temporários do usuário", roots: temp_roots, min_age_days: 2 }];
    if let Some(l) = var("LOCALAPPDATA") {
        cats.push(Category { id: "crash", name: "Relatórios de falha", roots: vec![l.join("CrashDumps")], min_age_days: 7 });
        cats.push(Category {
            id: "thumbs",
            name: "Cache de miniaturas",
            roots: vec![l.join("Microsoft").join("Windows").join("Explorer")],
            min_age_days: 0,
        });
    }
    cats
}

#[derive(Debug, Default, Clone)]
pub struct Plan {
    pub files: Vec<(PathBuf, u64)>,
    pub total_bytes: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub removed: usize,
    pub failed: usize,
    pub bytes: u64,
    pub dry_run: bool,
}

fn root_is_safe(root: &Path) -> bool {
    root.components().count() >= 3
}

/// Monta o plano (não altera nada). Na categoria `thumbs`, só casa `thumbcache_*.db`.
pub fn plan(cat: &Category, now: SystemTime) -> Plan {
    let mut plan = Plan::default();
    let cutoff = now.checked_sub(Duration::from_secs(cat.min_age_days * 86_400)).unwrap_or(SystemTime::UNIX_EPOCH);
    for root in &cat.roots {
        if !root_is_safe(root) {
            continue;
        }
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for item in rd.flatten() {
                let Ok(meta) = fs::symlink_metadata(item.path()) else { continue };
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    stack.push(item.path());
                    continue;
                }
                if cat.id == "thumbs" {
                    let n = item.file_name().to_string_lossy().to_lowercase();
                    if !(n.starts_with("thumbcache_") && n.ends_with(".db")) {
                        continue;
                    }
                }
                let old_enough = meta.modified().map(|m| m <= cutoff).unwrap_or(false);
                if old_enough {
                    plan.total_bytes += meta.len();
                    plan.files.push((item.path(), meta.len()));
                }
            }
        }
    }
    plan
}

/// Executa o plano. Com `dry_run = true` nada é removido.
pub fn execute(plan: &Plan, dry_run: bool) -> Report {
    let mut r = Report { dry_run, ..Default::default() };
    for (path, size) in &plan.files {
        if dry_run || fs::remove_file(path).is_ok() {
            r.removed += 1;
            r.bytes += size;
        } else {
            r.failed += 1;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn cat(root: &Path, age: u64) -> Category {
        Category { id: "temp", name: "t", roots: vec![root.to_path_buf()], min_age_days: age }
    }

    fn old_file(path: &Path, bytes: usize) {
        fs::write(path, vec![0u8; bytes]).unwrap();
        let f = File::options().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(10 * 86_400)).unwrap();
    }

    #[test]
    fn only_old_files_are_planned() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("a").join("b").join("tmp");
        fs::create_dir_all(root.join("sub")).unwrap();
        old_file(&root.join("sub/old.tmp"), 100);
        fs::write(root.join("new.tmp"), "x").unwrap();
        let p = plan(&cat(&root, 2), SystemTime::now());
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.total_bytes, 100);
    }

    #[test]
    fn dry_run_keeps_files_and_apply_removes() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("a").join("b").join("tmp");
        fs::create_dir_all(&root).unwrap();
        old_file(&root.join("x.tmp"), 10);
        let p = plan(&cat(&root, 1), SystemTime::now());
        let r = execute(&p, true);
        assert!(r.dry_run && r.removed == 1 && root.join("x.tmp").exists());
        let r = execute(&p, false);
        assert_eq!(r.removed, 1);
        assert!(!root.join("x.tmp").exists());
        assert!(root.exists(), "pastas nunca são removidas");
    }

    #[test]
    fn shallow_roots_are_refused() {
        assert!(!root_is_safe(Path::new("/")));
        assert!(!root_is_safe(Path::new("C:\\")));
        let p = plan(&cat(Path::new("/"), 0), SystemTime::now());
        assert!(p.files.is_empty());
    }
}
