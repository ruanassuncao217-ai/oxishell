//! Gerenciador de tarefas: instantâneo de processos, ordenação e encerramento.

use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcSort {
    Cpu,
    Memory,
    Name,
    Pid,
}

/// Processos que o Windows precisa para funcionar; encerrá-los derruba a sessão ou o sistema.
const PROTECTED: &[&str] = &[
    "system", "system idle process", "registry", "memory compression", "smss.exe", "csrss.exe", "wininit.exe",
    "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe", "dwm.exe", "fontdrvhost.exe", "lsaiso.exe",
    "sihost.exe", "ctfmon.exe",
];

/// `true` se o processo é crítico e o OxiShell se recusa a encerrá-lo.
pub fn is_protected(pid: u32, name: &str) -> bool {
    pid <= 4 || PROTECTED.contains(&name.to_lowercase().as_str())
}

pub struct Monitor {
    sys: System,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        sys.refresh_memory();
        sys.refresh_cpu_all();
        Monitor { sys }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        self.sys.refresh_memory();
        self.sys.refresh_cpu_all();
    }

    pub fn processes(&self, sort: ProcSort, filter: &str) -> Vec<ProcInfo> {
        let f = filter.to_lowercase();
        let mut v: Vec<ProcInfo> = self
            .sys
            .processes()
            .values()
            .map(|p| ProcInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage(),
                memory: p.memory(),
            })
            .filter(|p| f.is_empty() || p.name.to_lowercase().contains(&f) || p.pid.to_string() == f)
            .collect();
        sort_procs(&mut v, sort);
        v
    }

    /// (memória usada, total, uso médio de CPU em %)
    pub fn totals(&self) -> (u64, u64, f32) {
        (self.sys.used_memory(), self.sys.total_memory(), self.sys.global_cpu_usage())
    }

    /// Encerra um processo. Recusa o próprio processo.
    pub fn kill(&self, pid: u32) -> Result<(), String> {
        if pid == std::process::id() {
            return Err("não é possível encerrar o próprio OxiShell".into());
        }
        match self.sys.process(Pid::from_u32(pid)) {
            Some(p) => {
                if is_protected(pid, &p.name().to_string_lossy()) {
                    return Err("processo crítico do sistema: encerrar isso derrubaria o Windows".into());
                }
                if p.kill() {
                    Ok(())
                } else {
                    Err("o sistema recusou encerrar o processo (permissão?)".into())
                }
            }
            None => Err("processo não encontrado".into()),
        }
    }
}

pub fn sort_procs(v: &mut [ProcInfo], sort: ProcSort) {
    match sort {
        ProcSort::Cpu => v.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal)),
        ProcSort::Memory => v.sort_by_key(|p| std::cmp::Reverse(p.memory)),
        ProcSort::Name => v.sort_by_key(|p| p.name.to_lowercase()),
        ProcSort::Pid => v.sort_by_key(|p| p.pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str, cpu: f32, memory: u64) -> ProcInfo {
        ProcInfo { pid, name: name.into(), cpu, memory }
    }

    #[test]
    fn sorting() {
        let mut v = vec![p(2, "b", 1.0, 10), p(1, "A", 5.0, 5), p(3, "c", 3.0, 20)];
        sort_procs(&mut v, ProcSort::Cpu);
        assert_eq!(v[0].pid, 1);
        sort_procs(&mut v, ProcSort::Memory);
        assert_eq!(v[0].pid, 3);
        sort_procs(&mut v, ProcSort::Name);
        assert_eq!(v[0].name, "A");
        sort_procs(&mut v, ProcSort::Pid);
        assert_eq!(v[2].pid, 3);
    }

    #[test]
    fn critical_processes_are_protected() {
        assert!(is_protected(4, "System"));
        assert!(is_protected(900, "CSRSS.EXE"));
        assert!(is_protected(1234, "svchost.exe"));
        assert!(is_protected(0, "anything"));
        assert!(!is_protected(5000, "notepad.exe"));
    }

    #[test]
    fn sees_itself_and_refuses_self_kill() {
        let m = Monitor::new();
        let me = std::process::id();
        assert!(m.processes(ProcSort::Pid, "").iter().any(|p| p.pid == me));
        assert!(m.kill(me).is_err());
        assert!(m.kill(u32::MAX - 1).is_err());
        assert!(m.totals().1 > 0);
    }
}
