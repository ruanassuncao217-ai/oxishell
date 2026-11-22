use eframe::egui;
use oxi_core::{cleaner, diskusage, files, human_size, photos, procs, search, tune};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant, SystemTime};

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 680.0]).with_title("OxiShell"),
        ..Default::default()
    };
    eframe::run_native("OxiShell", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}

/// Tarefa em segundo plano com resultado único.
struct Job<T>(Option<Receiver<T>>);

impl<T: Send + 'static> Job<T> {
    fn idle() -> Self {
        Job(None)
    }
    fn running(&self) -> bool {
        self.0.is_some()
    }
    fn start(&mut self, f: impl FnOnce() -> T + Send + 'static) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        self.0 = Some(rx);
    }
    fn poll(&mut self) -> Option<T> {
        let r = self.0.as_ref()?.try_recv();
        match r {
            Ok(v) => {
                self.0 = None;
                Some(v)
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.0 = None;
                None
            }
            Err(_) => None,
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Files,
    Find,
    Tasks,
    Disk,
    Clean,
    Photos,
    Tune,
}

struct App {
    tab: Tab,
    files: FilesTab,
    find: FindTab,
    tasks: TasksTab,
    disk: DiskTab,
    clean: CleanTab,
    photos: PhotosTab,
    tune: TuneTab,
}

impl App {
    fn new() -> Self {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        App {
            tab: Tab::Files,
            files: FilesTab::new(home.clone()),
            find: FindTab::new(home.clone()),
            tasks: TasksTab::new(),
            disk: DiskTab::new(home.clone()),
            clean: CleanTab::new(),
            photos: PhotosTab::new(home),
            tune: TuneTab::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (t, label) in [
                    (Tab::Files, "Arquivos"),
                    (Tab::Find, "Busca"),
                    (Tab::Tasks, "Tarefas"),
                    (Tab::Disk, "Disco"),
                    (Tab::Clean, "Limpeza"),
                    (Tab::Photos, "Fotos"),
                    (Tab::Tune, "Ajustes"),
                ] {
                    ui.selectable_value(&mut self.tab, t, label);
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Files => self.files.ui(ui),
            Tab::Find => self.find.ui(ui),
            Tab::Tasks => self.tasks.ui(ui, ctx),
            Tab::Disk => self.disk.ui(ui, ctx),
            Tab::Clean => self.clean.ui(ui, ctx),
            Tab::Photos => self.photos.ui(ui, ctx),
            Tab::Tune => self.tune.ui(ui),
        });
    }
}

// ---------------------------------------------------------------- Arquivos

struct FilesTab {
    dir: PathBuf,
    path_text: String,
    entries: Vec<files::Entry>,
    selected: Option<PathBuf>,
    clipboard: Option<(PathBuf, bool)>, // (origem, é_recorte)
    show_hidden: bool,
    sort: files::SortKey,
    rename_text: String,
    new_dir: String,
    status: String,
}

impl FilesTab {
    fn new(dir: PathBuf) -> Self {
        let mut t = FilesTab {
            path_text: dir.display().to_string(),
            dir,
            entries: Vec::new(),
            selected: None,
            clipboard: None,
            show_hidden: false,
            sort: files::SortKey::Name,
            rename_text: String::new(),
            new_dir: String::new(),
            status: String::new(),
        };
        t.reload();
        t
    }

    fn reload(&mut self) {
        match files::list_dir(&self.dir, self.show_hidden, self.sort) {
            Ok(v) => {
                self.entries = v;
                self.status.clear();
            }
            Err(e) => {
                self.entries.clear();
                self.status = format!("Não foi possível abrir a pasta: {e}");
            }
        }
        self.path_text = self.dir.display().to_string();
        self.selected = None;
    }

    fn go(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.reload();
    }

    fn report<T>(&mut self, r: std::io::Result<T>, ok: &str) {
        match r {
            Ok(_) => {
                self.status = ok.to_string();
                self.reload();
            }
            Err(e) => self.status = format!("Erro: {e}"),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("⬆ Subir").clicked() {
                if let Some(p) = self.dir.parent() {
                    self.go(p.to_path_buf());
                }
            }
            let r = ui.add(egui::TextEdit::singleline(&mut self.path_text).desired_width(420.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let p = PathBuf::from(self.path_text.trim());
                if p.is_dir() {
                    self.go(p);
                } else {
                    self.status = "Pasta inexistente".into();
                }
            }
            if ui.checkbox(&mut self.show_hidden, "Ocultos").changed() {
                self.reload();
            }
            egui::ComboBox::from_id_salt("sort").selected_text(match self.sort {
                files::SortKey::Name => "Nome",
                files::SortKey::Size => "Tamanho",
                files::SortKey::Modified => "Modificado",
            }).show_ui(ui, |ui| {
                let mut ch = false;
                ch |= ui.selectable_value(&mut self.sort, files::SortKey::Name, "Nome").changed();
                ch |= ui.selectable_value(&mut self.sort, files::SortKey::Size, "Tamanho").changed();
                ch |= ui.selectable_value(&mut self.sort, files::SortKey::Modified, "Modificado").changed();
                if ch {
                    self.reload();
                }
            });
        });
        ui.horizontal(|ui| {
            let sel = self.selected.clone();
            ui.add_enabled_ui(sel.is_some(), |ui| {
                if ui.button("Copiar").clicked() {
                    self.clipboard = sel.clone().map(|p| (p, false));
                }
                if ui.button("Recortar").clicked() {
                    self.clipboard = sel.clone().map(|p| (p, true));
                }
                if ui.button("🗑 Lixeira").clicked() {
                    if let Some(p) = &sel {
                        let r = files::send_to_trash(p);
                        self.report(r, "Enviado para a lixeira");
                    }
                }
                ui.add(egui::TextEdit::singleline(&mut self.rename_text).hint_text("novo nome").desired_width(130.0));
                if ui.button("Renomear").clicked() {
                    if let Some(p) = &sel {
                        let r = files::rename(p, self.rename_text.trim());
                        self.report(r, "Renomeado");
                        self.rename_text.clear();
                    }
                }
            });
            if ui.add_enabled(self.clipboard.is_some(), egui::Button::new("Colar")).clicked() {
                if let Some((src, cut)) = self.clipboard.clone() {
                    let r = if cut { files::move_any(&src, &self.dir) } else { files::copy_any(&src, &self.dir) };
                    if cut && r.is_ok() {
                        self.clipboard = None;
                    }
                    self.report(r, "Concluído");
                }
            }
            ui.add(egui::TextEdit::singleline(&mut self.new_dir).hint_text("nova pasta").desired_width(110.0));
            if ui.button("Criar").clicked() {
                let r = files::make_dir(&self.dir, self.new_dir.trim());
                self.report(r, "Pasta criada");
                self.new_dir.clear();
            }
        });
        if !self.status.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_YELLOW, &self.status);
        }
        ui.separator();
        let mut open: Option<PathBuf> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for e in &self.entries {
                let label = format!("{} {}", if e.is_dir { "📁" } else { "📄" }, e.name);
                let selected = self.selected.as_ref() == Some(&e.path);
                ui.horizontal(|ui| {
                    let r = ui.selectable_label(selected, label);
                    if r.clicked() {
                        self.selected = Some(e.path.clone());
                    }
                    if r.double_clicked() && e.is_dir {
                        open = Some(e.path.clone());
                    }
                    if !e.is_dir {
                        ui.weak(human_size(e.size));
                    }
                });
            }
        });
        if let Some(p) = open {
            self.go(p);
        }
    }
}

// ------------------------------------------------------------------- Busca

struct FindTab {
    root_text: String,
    query: String,
    index: Option<search::Index>,
    job: Job<search::Index>,
    built_at: Option<Instant>,
}

impl FindTab {
    fn new(root: PathBuf) -> Self {
        FindTab { root_text: root.display().to_string(), query: String::new(), index: None, job: Job::idle(), built_at: None }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        if let Some(idx) = self.job.poll() {
            self.index = Some(idx);
            self.built_at = Some(Instant::now());
        }
        ui.horizontal(|ui| {
            ui.label("Pasta:");
            ui.add(egui::TextEdit::singleline(&mut self.root_text).desired_width(360.0));
            if ui.add_enabled(!self.job.running(), egui::Button::new("Indexar")).clicked() {
                let root = PathBuf::from(self.root_text.trim());
                self.job.start(move || search::Index::build(&[root], 3_000_000));
            }
            if self.job.running() {
                ui.spinner();
                ui.label("indexando…");
            } else if let Some(i) = &self.index {
                ui.label(format!("{} itens", i.entries.len()));
            }
        });
        ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("nome (ex.: relatorio ext:pdf, dir:, file:)").desired_width(f32::INFINITY));
        ui.separator();
        match &self.index {
            Some(idx) => {
                let results = idx.query(&self.query, 500);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for e in results {
                        ui.horizontal(|ui| {
                            ui.label(if e.is_dir { "📁" } else { "📄" });
                            ui.label(&e.name);
                            ui.weak(e.path.parent().map(|p| p.display().to_string()).unwrap_or_default());
                            if !e.is_dir {
                                ui.weak(human_size(e.size));
                            }
                        });
                    }
                });
            }
            None => {
                ui.weak("Escolha uma pasta e clique em Indexar para buscar.");
            }
        }
    }
}

// ----------------------------------------------------------------- Tarefas

struct TasksTab {
    monitor: procs::Monitor,
    sort: procs::ProcSort,
    filter: String,
    last: Instant,
    status: String,
}

impl TasksTab {
    fn new() -> Self {
        TasksTab { monitor: procs::Monitor::new(), sort: procs::ProcSort::Cpu, filter: String::new(), last: Instant::now(), status: String::new() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.last.elapsed() > Duration::from_millis(1500) {
            self.monitor.refresh();
            self.last = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_millis(1500));
        let (used, total, cpu) = self.monitor.totals();
        ui.horizontal(|ui| {
            ui.label(format!("CPU {cpu:.0}%"));
            ui.add(egui::ProgressBar::new(cpu / 100.0).desired_width(160.0));
            ui.label(format!("Memória {} / {}", human_size(used), human_size(total)));
            ui.add(egui::ProgressBar::new(if total > 0 { used as f32 / total as f32 } else { 0.0 }).desired_width(160.0));
        });
        ui.horizontal(|ui| {
            ui.label("Filtro:");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(160.0));
            for (s, l) in [(procs::ProcSort::Cpu, "CPU"), (procs::ProcSort::Memory, "Memória"), (procs::ProcSort::Name, "Nome"), (procs::ProcSort::Pid, "PID")] {
                ui.selectable_value(&mut self.sort, s, l);
            }
        });
        if !self.status.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_YELLOW, &self.status);
        }
        ui.separator();
        let list = self.monitor.processes(self.sort, &self.filter);
        let mut kill: Option<u32> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("procs").striped(true).num_columns(5).show(ui, |ui| {
                for h in ["PID", "Nome", "CPU", "Memória", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for p in list.iter().take(400) {
                    ui.label(p.pid.to_string());
                    ui.label(&p.name);
                    ui.label(format!("{:.1}%", p.cpu));
                    ui.label(human_size(p.memory));
                    if ui.small_button("Encerrar").clicked() {
                        kill = Some(p.pid);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(pid) = kill {
            self.status = match self.monitor.kill(pid) {
                Ok(()) => format!("Processo {pid} encerrado"),
                Err(e) => format!("Erro: {e}"),
            };
        }
    }
}

// ------------------------------------------------------------------- Disco

struct DiskTab {
    root_text: String,
    result: Option<diskusage::Node>,
    job: Job<std::io::Result<diskusage::Node>>,
    error: String,
}

impl DiskTab {
    fn new(root: PathBuf) -> Self {
        DiskTab { root_text: root.display().to_string(), result: None, job: Job::idle(), error: String::new() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(r) = self.job.poll() {
            match r {
                Ok(n) => {
                    self.result = Some(n);
                    self.error.clear();
                }
                Err(e) => self.error = format!("Erro: {e}"),
            }
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.root_text).desired_width(420.0));
            if ui.add_enabled(!self.job.running(), egui::Button::new("Analisar")).clicked() {
                let root = PathBuf::from(self.root_text.trim());
                self.job.start(move || diskusage::scan(&root, 6));
            }
            if self.job.running() {
                ui.spinner();
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
        }
        ui.separator();
        if let Some(root) = &self.result {
            ui.strong(format!("{} — {}", root.name, human_size(root.size)));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                draw_node(ui, root, root.size, 0);
            });
        } else {
            ui.weak("Escolha uma pasta e clique em Analisar.");
        }
    }
}

fn draw_node(ui: &mut egui::Ui, node: &diskusage::Node, parent_total: u64, depth: usize) {
    let frac = if parent_total > 0 { node.size as f32 / parent_total as f32 } else { 0.0 };
    let header = format!("{}{}  —  {}  ({:.1}%)", if node.is_dir { "📁 " } else { "📄 " }, node.name, human_size(node.size), frac * 100.0);
    if node.is_dir && !node.children.is_empty() && depth < 6 {
        egui::CollapsingHeader::new(header).id_salt((depth, &node.name, node.size)).show(ui, |ui| {
            ui.add(egui::ProgressBar::new(frac).desired_width(260.0));
            for c in node.children.iter().take(60) {
                draw_node(ui, c, node.size, depth + 1);
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.label(header);
        });
    }
}

// ----------------------------------------------------------------- Limpeza

struct CleanTab {
    plans: Vec<(cleaner::Category, cleaner::Plan, bool)>,
    message: String,
    confirm: bool,
}

impl CleanTab {
    fn new() -> Self {
        CleanTab { plans: Vec::new(), message: String::new(), confirm: false }
    }

    fn scan(&mut self) {
        self.plans = cleaner::default_categories()
            .into_iter()
            .map(|c| {
                let p = cleaner::plan(&c, SystemTime::now());
                (c, p, true)
            })
            .collect();
        self.message.clear();
        self.confirm = false;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.label("A limpeza só remove arquivos antigos de pastas temporárias. Primeiro veja o que seria removido.");
        if ui.button("Procurar arquivos para limpar").clicked() {
            self.scan();
        }
        let mut total = 0;
        for (c, p, on) in &mut self.plans {
            ui.horizontal(|ui| {
                ui.checkbox(on, c.name);
                ui.label(format!("{} arquivos, {}", p.files.len(), human_size(p.total_bytes)));
            });
            if *on {
                total += p.total_bytes;
            }
        }
        if !self.plans.is_empty() {
            ui.separator();
            ui.strong(format!("Total selecionado: {}", human_size(total)));
            ui.checkbox(&mut self.confirm, "Entendo que os arquivos serão removidos definitivamente");
            if ui.add_enabled(self.confirm && total > 0, egui::Button::new("Limpar agora")).clicked() {
                let mut freed = 0;
                let mut failed = 0;
                for (_, p, on) in &self.plans {
                    if *on {
                        let r = cleaner::execute(p, false);
                        freed += r.bytes;
                        failed += r.failed;
                    }
                }
                self.message = format!("Liberado {} ({} arquivos em uso foram ignorados)", human_size(freed), failed);
                self.plans.clear();
                self.confirm = false;
            }
        }
        if !self.message.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_GREEN, &self.message);
        }
    }
}

// ------------------------------------------------------------------- Fotos

struct PhotosTab {
    dir_text: String,
    list: Vec<PathBuf>,
    index: usize,
    shown: Option<usize>,
    texture: Option<egui::TextureHandle>,
    error: String,
}

impl PhotosTab {
    fn new(dir: PathBuf) -> Self {
        PhotosTab { dir_text: dir.display().to_string(), list: Vec::new(), index: 0, shown: None, texture: None, error: String::new() }
    }

    fn load(&mut self, ctx: &egui::Context) {
        let Some(path) = self.list.get(self.index) else { return };
        match image::open(path) {
            Ok(img) => {
                // Limita a 4096 px no maior lado para não estourar a textura.
                let img = if img.width().max(img.height()) > 4096 { img.resize(4096, 4096, image::imageops::FilterType::Triangle) } else { img };
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                self.texture = Some(ctx.load_texture("photo", color, egui::TextureOptions::LINEAR));
                self.error.clear();
            }
            Err(e) => {
                self.texture = None;
                self.error = format!("Não foi possível abrir a imagem: {e}");
            }
        }
        self.shown = Some(self.index);
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.dir_text).desired_width(380.0));
            if ui.button("Abrir pasta").clicked() {
                self.list = photos::images_in(&PathBuf::from(self.dir_text.trim()));
                self.index = 0;
                self.shown = None;
                if self.list.is_empty() {
                    self.error = "Nenhuma imagem encontrada".into();
                }
            }
            if !self.list.is_empty() {
                if ui.button("◀").clicked() || ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    self.index = photos::step(self.list.len(), self.index, false);
                }
                if ui.button("▶").clicked() || ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    self.index = photos::step(self.list.len(), self.index, true);
                }
                ui.label(format!("{}/{}  {}", self.index + 1, self.list.len(), self.list[self.index].file_name().unwrap_or_default().to_string_lossy()));
            }
        });
        if !self.list.is_empty() && self.shown != Some(self.index) {
            self.load(ctx);
        }
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
        }
        ui.separator();
        if let Some(tex) = &self.texture {
            let avail = ui.available_size();
            let size = tex.size_vec2();
            let scale = (avail.x / size.x).min(avail.y / size.y).clamp(0.01, 1.0);
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size * scale)));
            });
        }
    }
}

// ----------------------------------------------------------------- Ajustes

struct TuneTab {
    backup: tune::Backup,
    message: String,
}

impl TuneTab {
    fn new() -> Self {
        TuneTab { backup: tune::Backup::default(), message: String::new() }
    }

    #[cfg(windows)]
    fn ui(&mut self, ui: &mut egui::Ui) {
        use tune::windows_registry::WinRegistry;
        ui.label("Ajustes reversíveis, apenas no seu usuário (nada de sistema). Cada alteração guarda o valor original.");
        let mut reg = WinRegistry;
        let mut action: Option<(&'static str, bool)> = None;
        for t in tune::TWEAKS {
            let applied = tune::status(&reg, t) == tune::Status::Applied;
            ui.horizontal(|ui| {
                ui.label(if applied { "✅" } else { "⬜" });
                ui.vertical(|ui| {
                    ui.strong(t.title);
                    ui.weak(t.description);
                });
                if applied {
                    if ui.button("Desfazer").clicked() {
                        action = Some((t.id, false));
                    }
                } else if ui.button("Aplicar").clicked() {
                    action = Some((t.id, true));
                }
            });
        }
        if let Some((id, apply)) = action {
            let res = if apply { tune::apply(&mut reg, &[id], &mut self.backup, false) } else { tune::undo(&mut reg, &[id], &mut self.backup) };
            self.message = match res {
                Ok(log) => log.join("; "),
                Err(e) => format!("Erro: {e}"),
            };
        }
        if !self.message.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_GREEN, &self.message);
        }
        ui.weak("Dica: para desfazer em outra sessão, use `oxishell tune undo` (usa o mesmo backup).");
    }

    #[cfg(not(windows))]
    fn ui(&mut self, ui: &mut egui::Ui) {
        let _ = (&self.backup, &self.message);
        ui.label("Os ajustes do Windows só estão disponíveis no Windows.");
    }
}
