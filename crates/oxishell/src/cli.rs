//! Modo linha de comando: `oxishell clean|tune|find|disk|help`.
//! Operações que alteram algo exigem `--apply`; sem ele, é só simulação.

use oxi_core::{cleaner, diskusage, human_size, search, tune};
use std::path::Path;
use std::time::SystemTime;

const HELP: &str = "OxiShell — uso:
  oxishell                         abre a interface gráfica
  oxishell clean [--apply]         limpa temporários (simulação sem --apply)
  oxishell tune list               lista ajustes e se estão aplicados
  oxishell tune apply <id...|all> [--apply]
  oxishell tune undo [id...]       restaura os valores originais
  oxishell find <pasta> <consulta> busca por nome (ext:rs, dir:, file:)
  oxishell disk <pasta> [N]        maiores itens de uma pasta
";

pub fn run(args: &[String]) -> i32 {
    let apply = args.iter().any(|a| a == "--apply");
    let rest: Vec<&str> = args.iter().map(String::as_str).filter(|a| *a != "--apply").collect();
    match rest.as_slice() {
        ["clean"] => clean(!apply),
        ["tune", "list"] => tune_list(),
        ["tune", "apply", ids @ ..] if !ids.is_empty() => tune_apply(ids, !apply),
        ["tune", "undo", ids @ ..] => tune_undo(ids),
        ["find", dir, q @ ..] if !q.is_empty() => find(Path::new(dir), &q.join(" ")),
        ["disk", dir] => disk(Path::new(dir), 15),
        ["disk", dir, n] => disk(Path::new(dir), n.parse().unwrap_or(15)),
        ["help"] | ["--help"] | ["-h"] => {
            print!("{HELP}");
            0
        }
        _ => {
            eprint!("{HELP}");
            2
        }
    }
}

fn clean(dry_run: bool) -> i32 {
    let mut total = 0;
    for cat in cleaner::default_categories() {
        let plan = cleaner::plan(&cat, SystemTime::now());
        let r = cleaner::execute(&plan, dry_run);
        println!("{:<38} {:>6} arquivos  {:>10}", cat.name, r.removed, human_size(r.bytes));
        if r.failed > 0 {
            println!("  ({} arquivos em uso foram ignorados)", r.failed);
        }
        total += r.bytes;
    }
    println!("{} {}", if dry_run { "Liberaria (simulação; use --apply):" } else { "Liberado:" }, human_size(total));
    0
}

fn load_backup() -> Result<tune::Backup, String> {
    tune::Backup::load(&tune::default_backup_path())
}

fn save_backup(b: &tune::Backup) -> Result<(), String> {
    b.save(&tune::default_backup_path())
}

#[cfg(windows)]
fn registry() -> Option<tune::windows_registry::WinRegistry> {
    Some(tune::windows_registry::WinRegistry)
}

#[cfg(not(windows))]
fn registry() -> Option<NoRegistry> {
    None
}

#[cfg(not(windows))]
struct NoRegistry;
#[cfg(not(windows))]
impl tune::Registry for NoRegistry {
    fn get(&self, _: &str, _: &str) -> Option<u32> {
        None
    }
    fn set(&mut self, _: &str, _: &str, _: u32) -> Result<(), String> {
        Err("ajustes só funcionam no Windows".into())
    }
    fn delete(&mut self, _: &str, _: &str) -> Result<(), String> {
        Err("ajustes só funcionam no Windows".into())
    }
}

fn tune_list() -> i32 {
    let Some(reg) = registry() else {
        eprintln!("Os ajustes só estão disponíveis no Windows.");
        return 1;
    };
    for t in tune::TWEAKS {
        let mark = if tune::status(&reg, t) == tune::Status::Applied { "[x]" } else { "[ ]" };
        println!("{mark} {:<18} {}\n      {}", t.id, t.title, t.description);
    }
    0
}

fn tune_apply(ids: &[&str], dry_run: bool) -> i32 {
    let Some(mut reg) = registry() else {
        eprintln!("Os ajustes só estão disponíveis no Windows.");
        return 1;
    };
    let all: Vec<&str> = tune::TWEAKS.iter().map(|t| t.id).collect();
    let ids: Vec<&str> = if ids == ["all"] { all } else { ids.to_vec() };
    let mut backup = match load_backup() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("erro: {e}");
            return 1;
        }
    };
    match tune::apply(&mut reg, &ids, &mut backup, dry_run) {
        Ok(log) => {
            log.iter().for_each(|l| println!("{l}"));
            if dry_run {
                println!("Simulação: nada foi alterado. Use --apply para aplicar.");
            } else if let Err(e) = save_backup(&backup) {
                eprintln!("aviso: não consegui salvar o backup: {e}");
                return 1;
            }
            0
        }
        Err(e) => {
            eprintln!("erro: {e}");
            1
        }
    }
}

fn tune_undo(ids: &[&str]) -> i32 {
    let Some(mut reg) = registry() else {
        eprintln!("Os ajustes só estão disponíveis no Windows.");
        return 1;
    };
    let mut backup = match load_backup() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("erro: {e}");
            return 1;
        }
    };
    match tune::undo(&mut reg, ids, &mut backup) {
        Ok(log) => {
            log.iter().for_each(|l| println!("{l}"));
            if let Err(e) = save_backup(&backup) {
                eprintln!("aviso: {e}");
            }
            0
        }
        Err(e) => {
            eprintln!("erro: {e}");
            1
        }
    }
}

fn find(dir: &Path, q: &str) -> i32 {
    let idx = search::Index::build(&[dir.to_path_buf()], 2_000_000);
    println!("{} itens indexados", idx.entries.len());
    for e in idx.query(q, 50) {
        println!("{}{}", e.path.display(), if e.is_dir { std::path::MAIN_SEPARATOR_STR } else { "" });
    }
    0
}

fn disk(dir: &Path, n: usize) -> i32 {
    match diskusage::scan(dir, 6) {
        Ok(root) => {
            println!("Total: {}", human_size(root.size));
            for c in root.children.iter().take(n) {
                println!("{:>10}  {}{}", human_size(c.size), c.name, if c.is_dir { "/" } else { "" });
            }
            0
        }
        Err(e) => {
            eprintln!("erro: {e}");
            1
        }
    }
}
