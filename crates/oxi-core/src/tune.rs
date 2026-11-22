//! Ajustes de privacidade e usabilidade, todos reversíveis.
//!
//! Só mexe em valores DWORD de `HKEY_CURRENT_USER` (nada de sistema, serviços ou
//! políticas). Antes de aplicar, guarda o valor anterior num arquivo de backup;
//! `undo` restaura (ou apaga o valor que não existia).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct Tweak {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Caminho sob HKEY_CURRENT_USER.
    pub key: &'static str,
    pub value: &'static str,
    pub set_to: u32,
}

pub const TWEAKS: &[Tweak] = &[
    Tweak {
        id: "ad-id",
        title: "Desativar ID de publicidade",
        description: "Impede que apps usem um identificador para anúncios personalizados.",
        key: r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
        value: "Enabled",
        set_to: 0,
    },
    Tweak {
        id: "tips",
        title: "Desativar dicas e sugestões do Windows",
        description: "Remove sugestões de apps e dicas exibidas no menu Iniciar e nas configurações.",
        key: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        value: "SubscribedContent-338389Enabled",
        set_to: 0,
    },
    Tweak {
        id: "start-suggestions",
        title: "Desativar apps sugeridos no Iniciar",
        description: "Para de instalar e mostrar apps sugeridos.",
        key: r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        value: "SystemPaneSuggestionsEnabled",
        set_to: 0,
    },
    Tweak {
        id: "bing-search",
        title: "Desativar Bing na pesquisa do Iniciar",
        description: "A pesquisa do Iniciar passa a procurar só no computador.",
        key: r"Software\Microsoft\Windows\CurrentVersion\Search",
        value: "BingSearchEnabled",
        set_to: 0,
    },
    Tweak {
        id: "file-ext",
        title: "Mostrar extensões de arquivo",
        description: "Exibe .exe, .pdf etc. no Explorer (ajuda a identificar arquivos disfarçados).",
        key: r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        value: "HideFileExt",
        set_to: 0,
    },
];

pub fn find(id: &str) -> Option<&'static Tweak> {
    TWEAKS.iter().find(|t| t.id == id)
}

/// Abstração do registro, para permitir testes fora do Windows.
pub trait Registry {
    fn get(&self, key: &str, name: &str) -> Option<u32>;
    fn set(&mut self, key: &str, name: &str, value: u32) -> Result<(), String>;
    fn delete(&mut self, key: &str, name: &str) -> Result<(), String>;
}

/// Valor anterior de cada ajuste aplicado (`None` = o valor não existia).
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Backup {
    pub previous: BTreeMap<String, Option<u32>>,
}

impl Backup {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    pub fn from_json(s: &str) -> Result<Backup, String> {
        serde_json::from_str(s).map_err(|e| format!("backup inválido: {e}"))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Applied,
    NotApplied,
}

pub fn status(reg: &dyn Registry, t: &Tweak) -> Status {
    if reg.get(t.key, t.value) == Some(t.set_to) {
        Status::Applied
    } else {
        Status::NotApplied
    }
}

/// Aplica os ajustes `ids`. Se `dry_run`, só devolve o que seria alterado.
/// O backup só registra o valor original na primeira aplicação de cada ajuste.
pub fn apply(reg: &mut dyn Registry, ids: &[&str], backup: &mut Backup, dry_run: bool) -> Result<Vec<String>, String> {
    let mut log = Vec::new();
    for id in ids {
        let t = find(id).ok_or_else(|| format!("ajuste desconhecido: {id}"))?;
        let before = reg.get(t.key, t.value);
        if before == Some(t.set_to) {
            log.push(format!("{}: já aplicado", t.id));
            continue;
        }
        log.push(format!("{}: {} -> {}", t.id, before.map(|v| v.to_string()).unwrap_or_else(|| "(ausente)".into()), t.set_to));
        if !dry_run {
            backup.previous.entry(t.id.to_string()).or_insert(before);
            reg.set(t.key, t.value, t.set_to)?;
        }
    }
    Ok(log)
}

/// Desfaz os ajustes presentes no backup (todos, ou só `ids` se não vazio).
pub fn undo(reg: &mut dyn Registry, ids: &[&str], backup: &mut Backup) -> Result<Vec<String>, String> {
    let mut log = Vec::new();
    let targets: Vec<String> = if ids.is_empty() {
        backup.previous.keys().cloned().collect()
    } else {
        ids.iter().map(|s| s.to_string()).collect()
    };
    for id in targets {
        let Some(t) = find(&id) else { continue };
        let Some(prev) = backup.previous.get(&id).copied() else {
            log.push(format!("{id}: sem backup, nada a desfazer"));
            continue;
        };
        match prev {
            Some(v) => reg.set(t.key, t.value, v)?,
            None => reg.delete(t.key, t.value)?,
        }
        backup.previous.remove(&id);
        log.push(format!("{id}: restaurado"));
    }
    Ok(log)
}

#[cfg(windows)]
pub mod windows_registry {
    use super::Registry;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::RegKey;

    /// Registro real do Windows (somente HKEY_CURRENT_USER).
    pub struct WinRegistry;

    impl Registry for WinRegistry {
        fn get(&self, key: &str, name: &str) -> Option<u32> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            hkcu.open_subkey_with_flags(key, KEY_READ).ok()?.get_value::<u32, _>(name).ok()
        }
        fn set(&mut self, key: &str, name: &str, value: u32) -> Result<(), String> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (k, _) = hkcu.create_subkey(key).map_err(|e| e.to_string())?;
            k.set_value(name, &value).map_err(|e| e.to_string())
        }
        fn delete(&mut self, key: &str, name: &str) -> Result<(), String> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            match hkcu.open_subkey_with_flags(key, KEY_SET_VALUE) {
                Ok(k) => match k.delete_value(name) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.to_string()),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake(HashMap<(String, String), u32>);

    impl Registry for Fake {
        fn get(&self, k: &str, n: &str) -> Option<u32> {
            self.0.get(&(k.into(), n.into())).copied()
        }
        fn set(&mut self, k: &str, n: &str, v: u32) -> Result<(), String> {
            self.0.insert((k.into(), n.into()), v);
            Ok(())
        }
        fn delete(&mut self, k: &str, n: &str) -> Result<(), String> {
            self.0.remove(&(k.into(), n.into()));
            Ok(())
        }
    }

    #[test]
    fn ids_are_unique_and_keys_are_user_scoped() {
        let mut ids: Vec<_> = TWEAKS.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), TWEAKS.len());
        assert!(TWEAKS.iter().all(|t| t.key.starts_with("Software\\")));
    }

    #[test]
    fn dry_run_changes_nothing() {
        let mut reg = Fake::default();
        let mut b = Backup::default();
        let log = apply(&mut reg, &["ad-id"], &mut b, true).unwrap();
        assert_eq!(log.len(), 1);
        assert!(reg.0.is_empty() && b.previous.is_empty());
    }

    #[test]
    fn apply_then_undo_restores_original_state() {
        let t = find("file-ext").unwrap();
        let mut reg = Fake::default();
        reg.set(t.key, t.value, 1).unwrap(); // valor existente
        let mut b = Backup::default();
        apply(&mut reg, &["file-ext", "ad-id"], &mut b, false).unwrap();
        assert_eq!(status(&reg, t), Status::Applied);
        assert_eq!(b.previous["file-ext"], Some(1));
        assert_eq!(b.previous["ad-id"], None);

        // backup sobrevive à serialização
        let mut b2 = Backup::from_json(&b.to_json()).unwrap();
        assert_eq!(b, b2);
        undo(&mut reg, &[], &mut b2).unwrap();
        assert_eq!(reg.get(t.key, t.value), Some(1));
        let ad = find("ad-id").unwrap();
        assert_eq!(reg.get(ad.key, ad.value), None, "valor ausente volta a ausente");
        assert!(b2.previous.is_empty());
    }

    #[test]
    fn reapplying_keeps_first_original() {
        let t = find("ad-id").unwrap();
        let mut reg = Fake::default();
        reg.set(t.key, t.value, 1).unwrap();
        let mut b = Backup::default();
        apply(&mut reg, &["ad-id"], &mut b, false).unwrap();
        let log = apply(&mut reg, &["ad-id"], &mut b, false).unwrap();
        assert!(log[0].contains("já aplicado"));
        assert_eq!(b.previous["ad-id"], Some(1));
    }

    #[test]
    fn unknown_id_is_an_error() {
        let mut reg = Fake::default();
        assert!(apply(&mut reg, &["nope"], &mut Backup::default(), false).is_err());
    }
}
