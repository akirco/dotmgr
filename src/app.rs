use crate::config;
use crate::utils::{format_size, hash_file_quick};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mirror_exists: bool,
    pub has_diff: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum IgnoreStatus {
    NotIgnored,
    DirectlyIgnored,
    InheritedIgnored,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrowseMode {
    Home,
    Sync,
}

#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub copied: u64,
    pub skipped: u64,
    pub cleaned: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DeployStats {
    pub copied: u64,
    pub skipped: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
    pub overwritten: u64,
}

#[derive(Debug, Clone)]
pub enum BackgroundEvent {
    SyncDone(SyncStats),
    DeployDone(DeployStats),
    Error(String),
}

pub struct App {
    pub home_dir: PathBuf,
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,

    pub ignored: HashSet<String>,
    pub config: config::AppConfig,

    pub show_all: bool,
    pub show_syncable_only: bool,
    pub browse_mode: BrowseMode,

    pub full_count: usize,
    pub full_tracked: usize,
    pub full_ignored: usize,

    pub status: String,
    pub should_quit: bool,
    pub awaiting_confirm: Option<String>,

    pub pending: bool,
    pub tick: u64,
    pub tx: mpsc::UnboundedSender<BackgroundEvent>,
    pub rx: mpsc::UnboundedReceiver<BackgroundEvent>,

}

impl App {
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let config = config::load_config();
        let ignored_paths = config::load_ignores();
        let mut ignored: HashSet<String> = ignored_paths.into_iter().collect();
        if let Ok(rel) = config.sync_dir.strip_prefix(&home_dir) {
            ignored.insert(rel.to_string_lossy().to_string());
        }
        let (tx, rx) = mpsc::unbounded_channel();
        let mut app = Self {
            current_dir: home_dir.clone(),
            home_dir,
            entries: Vec::new(),
            selected: 0,
            ignored,
            config,
            show_all: false,
            show_syncable_only: false,
            browse_mode: BrowseMode::Home,
            full_count: 0,
            full_tracked: 0,
            full_ignored: 0,
            status: String::new(),
            should_quit: false,
            awaiting_confirm: None,
            pending: false,
            tick: 0,
            tx,
            rx,

        };
        app.load_entries();
        app
    }

    pub fn handle_background_event(&mut self, event: BackgroundEvent) {
        match event {
            BackgroundEvent::SyncDone(stats) => {
                self.pending = false;
                self.load_entries();
                self.status = format_sync_status(&stats);
            }
            BackgroundEvent::DeployDone(stats) => {
                self.pending = false;
                self.load_entries();
                self.status = format_deploy_status(&stats);
            }
            BackgroundEvent::Error(msg) => {
                self.pending = false;
                self.status = msg;
            }
        }
    }

    pub fn base_dir(&self) -> &Path {
        match self.browse_mode {
            BrowseMode::Home => &self.home_dir,
            BrowseMode::Sync => &self.config.sync_dir,
        }
    }

    pub fn relative_path(&self, path: &Path) -> String {
        if let Ok(rel) = path.strip_prefix(&self.config.sync_dir) {
            return rel.to_string_lossy().to_string();
        }
        if let Ok(rel) = path.strip_prefix(&self.home_dir) {
            return rel.to_string_lossy().to_string();
        }
        path.to_string_lossy().to_string()
    }

    pub fn display_path(&self) -> String {
        let base = self.base_dir();
        let rel = self
            .current_dir
            .strip_prefix(base)
            .unwrap_or(Path::new(""))
            .to_string_lossy();

        match self.browse_mode {
            BrowseMode::Home => {
                if rel.is_empty() {
                    "~/".into()
                } else {
                    format!("~/{}", rel)
                }
            }
            BrowseMode::Sync => {
                if rel.is_empty() {
                    "sync:/".into()
                } else {
                    format!("sync:/{}", rel)
                }
            }
        }
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        let rel = self.relative_path(path);
        let mut p = PathBuf::from(&rel);
        loop {
            if self.ignored.contains(&p.to_string_lossy().to_string()) {
                return true;
            }
            if !p.pop() {
                break;
            }
        }
        false
    }

    pub fn is_directly_ignored(&self, path: &Path) -> bool {
        let rel = self.relative_path(path);
        self.ignored.contains(&rel)
    }

    pub fn ignore_status(&self, path: &Path) -> IgnoreStatus {
        if self.is_directly_ignored(path) {
            return IgnoreStatus::DirectlyIgnored;
        }
        if self.is_ignored(path) {
            return IgnoreStatus::InheritedIgnored;
        }
        IgnoreStatus::NotIgnored
    }

    pub fn toggle_browse_mode(&mut self) {
        let old_base = self.base_dir();
        let rel = self
            .current_dir
            .strip_prefix(old_base)
            .unwrap_or(Path::new(""))
            .to_path_buf();

        self.browse_mode = match self.browse_mode {
            BrowseMode::Home => BrowseMode::Sync,
            BrowseMode::Sync => BrowseMode::Home,
        };

        let new_base = self.base_dir().to_path_buf();
        self.current_dir = new_base.join(&rel);

        if !self.current_dir.exists() || !self.current_dir.is_dir() {
            self.current_dir = new_base.to_path_buf();
        }

        self.selected = 0;
        self.load_entries();
    }

    pub fn load_entries(&mut self) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        if let Ok(read_dir) = fs::read_dir(&self.current_dir) {
            for entry in read_dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();

                if name == "." || name == ".." {
                    continue;
                }

                if self.browse_mode == BrowseMode::Home
                    && self.current_dir == self.home_dir
                    && self
                        .config
                        .sync_dir
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy() == name)
                {
                    continue;
                }

                if self.browse_mode == BrowseMode::Home
                    && !self.show_all
                    && self.current_dir == self.home_dir
                    && !name.starts_with('.')
                {
                    continue;
                }

                if self.browse_mode == BrowseMode::Sync
                    && !self.show_all
                    && self.current_dir == self.config.sync_dir
                    && self.config.synced_ignores.contains(&name)
                {
                    continue;
                }

                if self.browse_mode == BrowseMode::Sync
                    && !self.show_all
                    && self.current_dir == self.config.sync_dir
                    && !name.starts_with('.')
                {
                    continue;
                }

                let path = entry.path();
                let is_symlink = path.is_symlink();
                let is_dir = path.is_dir();
                let size = if is_dir {
                    0
                } else {
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                };

                let rel = self.relative_path(&path);
                let mirror_path = match self.browse_mode {
                    BrowseMode::Home => self.config.sync_dir.join(&rel),
                    BrowseMode::Sync => self.home_dir.join(&rel),
                };
                let mirror_exists = mirror_path.exists();
                let has_diff = !is_dir && mirror_exists && self.has_file_diff(&path, &mirror_path);

                let fe = FileEntry {
                    name,
                    path,
                    is_dir,
                    is_symlink,
                    size,
                    mirror_exists,
                    has_diff,
                };

                if is_dir {
                    dirs.push(fe);
                } else {
                    files.push(fe);
                }
            }
        }

        dirs.sort_by_key(|a| a.name.to_lowercase());
        files.sort_by_key(|a| a.name.to_lowercase());

        let mut all_entries = dirs;
        all_entries.extend(files);

        self.full_count = all_entries.len();
        self.full_tracked = all_entries
            .iter()
            .filter(|e| !self.is_ignored(&e.path))
            .count();
        self.full_ignored = all_entries
            .iter()
            .filter(|e| self.is_ignored(&e.path))
            .count();

        if self.show_syncable_only {
            all_entries.retain(|e| !self.is_ignored(&e.path));
        }

        self.entries = all_entries;
    }

    pub fn move_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = if self.selected > 0 {
            self.selected - 1
        } else {
            self.entries.len() - 1
        };
    }

    pub fn move_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = if self.selected < self.entries.len().saturating_sub(1) {
            self.selected + 1
        } else {
            0
        };
    }

    pub fn enter_directory(&mut self) -> Option<PathBuf> {
        let entry = self.entries.get(self.selected)?;
        if entry.is_dir {
            self.current_dir = entry.path.clone();
            self.selected = 0;
            self.load_entries();
            None
        } else {
            Some(entry.path.clone())
        }
    }

    pub fn go_back(&mut self) {
        if self.awaiting_confirm.is_some() {
            self.awaiting_confirm = None;
            self.status.clear();
            return;
        }

        let base = self.base_dir();
        if self.current_dir == base {
            return;
        }

        let child_name = self
            .current_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.load_entries();

            for (i, e) in self.entries.iter().enumerate() {
                if e.name == child_name {
                    self.selected = i;
                    break;
                }
            }
        }
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        self.selected = self.entries.len().saturating_sub(1);
    }

    pub fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        self.selected = (self.selected + 10).min(self.entries.len().saturating_sub(1));
    }

    pub fn toggle_ignore(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            let rel = self.relative_path(&entry.path);

            if self.ignored.contains(&rel) {
                self.ignored.remove(&rel);
            } else {
                self.ignored.insert(rel.clone());
            }
            self.persist_ignores();

            let old_name = entry.name.clone();
            self.load_entries();

            for (i, e) in self.entries.iter().enumerate() {
                if e.name == old_name {
                    self.selected = i;
                    break;
                }
            }
        }
    }

    fn persist_ignores(&self) {
        let list: Vec<String> = self.ignored.iter().cloned().collect();
        config::save_ignores(&list);
    }

    pub fn toggle_show_all(&mut self) {
        self.show_all = !self.show_all;
        self.selected = 0;
        self.load_entries();
    }

    pub fn toggle_syncable_only(&mut self) {
        self.show_syncable_only = !self.show_syncable_only;
        self.selected = 0;
        self.load_entries();
    }

    pub fn ignore_all(&mut self) {
        for entry in &self.entries {
            let rel = self.relative_path(&entry.path);
            if !self.ignored.contains(&rel) {
                self.ignored.insert(rel);
            }
        }
        self.persist_ignores();
        self.load_entries();
        self.status = format!("Ignored {} items", self.entries.len());
    }

    pub fn refresh(&mut self) {
        self.load_entries();
    }


    pub fn request_confirm(&mut self, action: &str) {
        self.awaiting_confirm = Some(action.to_string());
        match action {
            "sync_all" => self.status = "Sync ALL to repo? (y/N)".into(),
            "deploy_all" => self.status = "Deploy ALL to home? (y/N)".into(),
            _ => {}
        }
    }

    pub fn confirm_action(&mut self) {
        let action = self.awaiting_confirm.take();
        match action.as_deref() {
            Some("sync_all") => self.sync_all(),
            Some("deploy_all") => self.deploy_all(),
            _ => self.status.clear(),
        }
    }

    pub fn cancel_confirm(&mut self) {
        self.awaiting_confirm = None;
        self.status = "Cancelled".into();
    }

    pub fn sync_selected(&mut self) {
        if self.pending {
            return;
        }
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        let rel = self.relative_path(&entry.path);
        let src_path = self.home_dir.join(&rel);
        let dest_path = self.config.sync_dir.join(&rel);

        if !src_path.exists() {
            self.status = format!("Not in home: {}", rel);
            return;
        }

        if self.is_ignored(&src_path) {
            self.status = format!("Ignored: {}", rel);
            return;
        }

        self.pending = true;
        self.status = format!("Syncing {}...", rel);

        let ctx = WorkerCtx {
            home_dir: self.home_dir.clone(),
            sync_dir: self.config.sync_dir.clone(),
            ignored: self.ignored.clone(),
            synced_ignores: self.config.synced_ignores.clone(),
            show_all: self.show_all,
        };
        let is_dir = entry.is_dir;
        let tx = self.tx.clone();

        tokio::task::spawn_blocking(move || {
            let mut stats = SyncStats::default();
            if is_dir {
                let _ = fs::create_dir_all(&dest_path);
                stats.dirs += 1;
                if let Err(e) = worker_sync_walk(&src_path, &ctx, &mut stats) {
                    let _ = tx.send(BackgroundEvent::Error(format!("Sync FAILED: {}", e)));
                    return;
                }
                worker_clean_walk(&dest_path, &ctx, &mut stats);
            } else {
                worker_sync_file(&src_path, &dest_path, &mut stats);
            }
            let _ = tx.send(BackgroundEvent::SyncDone(stats));
        });
    }

    pub fn sync_all(&mut self) {
        if self.pending {
            return;
        }

        let sync_dir = self.config.sync_dir.clone();
        let ctx = WorkerCtx {
            home_dir: self.home_dir.clone(),
            sync_dir: sync_dir.clone(),
            ignored: self.ignored.clone(),
            synced_ignores: self.config.synced_ignores.clone(),
            show_all: self.show_all,
        };
        let tx = self.tx.clone();

        self.pending = true;
        self.status = "Syncing all...".into();

        tokio::task::spawn_blocking(move || {
            if let Err(e) = fs::create_dir_all(&sync_dir) {
                let _ = tx.send(BackgroundEvent::Error(format!("Error: {}", e)));
                return;
            }

            let mut stats = SyncStats::default();
            if let Err(e) = worker_sync_walk(&ctx.home_dir, &ctx, &mut stats) {
                let _ = tx.send(BackgroundEvent::Error(format!("Sync FAILED: {}", e)));
                return;
            }
            worker_clean_walk(&ctx.sync_dir, &ctx, &mut stats);
            let _ = tx.send(BackgroundEvent::SyncDone(stats));
        });
    }

    pub fn deploy_selected(&mut self) {
        if self.pending {
            return;
        }
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        let rel = self.relative_path(&entry.path);
        let src_path = self.config.sync_dir.join(&rel);
        let dest_path = self.home_dir.join(&rel);

        if !src_path.exists() {
            self.status = format!("Not in sync repo: {}", rel);
            return;
        }

        self.pending = true;
        self.status = format!("Deploying {}...", rel);

        let ctx = WorkerCtx {
            home_dir: self.home_dir.clone(),
            sync_dir: self.config.sync_dir.clone(),
            ignored: self.ignored.clone(),
            synced_ignores: self.config.synced_ignores.clone(),
            show_all: self.show_all,
        };
        let is_dir = entry.is_dir;
        let tx = self.tx.clone();

        tokio::task::spawn_blocking(move || {
            let mut stats = DeployStats::default();
            if is_dir {
                let _ = fs::create_dir_all(&dest_path);
                stats.dirs += 1;
                if let Err(e) = worker_deploy_walk(&src_path, &ctx, &mut stats) {
                    let _ = tx.send(BackgroundEvent::Error(format!("Deploy FAILED: {}", e)));
                    return;
                }
            } else {
                worker_deploy_file(&src_path, &dest_path, &mut stats);
            }
            let _ = tx.send(BackgroundEvent::DeployDone(stats));
        });
    }

    pub fn deploy_all(&mut self) {
        if self.pending {
            return;
        }

        let sync_dir = self.config.sync_dir.clone();
        if !sync_dir.exists() {
            self.status = "Nothing to deploy: sync dir does not exist".into();
            return;
        }

        let ctx = WorkerCtx {
            home_dir: self.home_dir.clone(),
            sync_dir: sync_dir.clone(),
            ignored: self.ignored.clone(),
            synced_ignores: self.config.synced_ignores.clone(),
            show_all: self.show_all,
        };
        let tx = self.tx.clone();

        self.pending = true;
        self.status = "Deploying all...".into();

        tokio::task::spawn_blocking(move || {
            let mut stats = DeployStats::default();
            if let Err(e) = worker_deploy_walk(&ctx.sync_dir, &ctx, &mut stats) {
                let _ = tx.send(BackgroundEvent::Error(format!("Deploy FAILED: {}", e)));
                return;
            }
            let _ = tx.send(BackgroundEvent::DeployDone(stats));
        });
    }

    fn has_file_diff(&self, path_a: &Path, path_b: &Path) -> bool {
        let meta_a = match fs::metadata(path_a) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let meta_b = match fs::metadata(path_b) {
            Ok(m) => m,
            Err(_) => return false,
        };

        if meta_a.len() != meta_b.len() {
            return true;
        }

        if let (Ok(hash_a), Ok(hash_b)) = (
            hash_file_quick(path_a, meta_a.len()),
            hash_file_quick(path_b, meta_b.len()),
        ) {
            return hash_a != hash_b;
        }

        false
    }
}

pub fn format_sync_status(stats: &SyncStats) -> String {
    let mut parts = Vec::new();
    if stats.copied > 0 {
        parts.push(format!(
            "↑ copied {} ({})",
            stats.copied,
            format_size(stats.bytes)
        ));
    }
    if stats.skipped > 0 {
        parts.push(format!("up-to-date {}", stats.skipped));
    }
    if stats.dirs > 0 {
        parts.push(format!("dirs {}", stats.dirs));
    }
    if stats.cleaned > 0 {
        parts.push(format!("cleaned {}", stats.cleaned));
    }
    if stats.errors > 0 {
        parts.push(format!("errors {}", stats.errors));
    }
    if parts.is_empty() {
        "Already up to date ✓".into()
    } else {
        parts.join(" │ ")
    }
}

pub fn format_deploy_status(stats: &DeployStats) -> String {
    let mut parts = Vec::new();
    if stats.copied > 0 {
        parts.push(format!(
            "↓ deployed {} ({})",
            stats.copied,
            format_size(stats.bytes)
        ));
    }
    if stats.overwritten > 0 {
        parts.push(format!("overwritten {}", stats.overwritten));
    }
    if stats.skipped > 0 {
        parts.push(format!("up-to-date {}", stats.skipped));
    }
    if stats.dirs > 0 {
        parts.push(format!("dirs {}", stats.dirs));
    }
    if stats.errors > 0 {
        parts.push(format!("errors {}", stats.errors));
    }
    if parts.is_empty() {
        "Already up to date ✓".into()
    } else {
        parts.join(" │ ")
    }
}

#[derive(Clone)]
struct WorkerCtx {
    home_dir: PathBuf,
    sync_dir: PathBuf,
    ignored: HashSet<String>,
    synced_ignores: Vec<String>,
    show_all: bool,
}

fn worker_relative_path(path: &Path, home_dir: &Path, sync_dir: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(sync_dir) {
        return rel.to_string_lossy().to_string();
    }
    if let Ok(rel) = path.strip_prefix(home_dir) {
        return rel.to_string_lossy().to_string();
    }
    path.to_string_lossy().to_string()
}

fn worker_is_ignored(path: &Path, ctx: &WorkerCtx) -> bool {
    let rel = worker_relative_path(path, &ctx.home_dir, &ctx.sync_dir);
    let mut p = PathBuf::from(&rel);
    loop {
        if ctx.ignored.contains(&p.to_string_lossy().to_string()) {
            return true;
        }
        if !p.pop() {
            break;
        }
    }
    false
}

fn worker_sync_file(src_path: &Path, dest_path: &Path, stats: &mut SyncStats) {
    #[cfg(unix)]
    {
        if src_path.is_symlink() {
            if let Ok(target) = fs::read_link(src_path) {
                if dest_path.is_symlink()
                    && let Ok(existing_target) = fs::read_link(dest_path)
                    && existing_target == target
                {
                    stats.skipped += 1;
                    return;
                }
                if dest_path.exists() || dest_path.is_symlink() {
                    let _ = fs::remove_file(dest_path);
                }
                match std::os::unix::fs::symlink(&target, dest_path) {
                    Ok(_) => stats.copied += 1,
                    Err(_) => stats.errors += 1,
                }
            }
            return;
        }
    }

    let src_meta = match fs::metadata(src_path) {
        Ok(m) => m,
        Err(_) => {
            stats.errors += 1;
            return;
        }
    };

    let should_copy = match fs::metadata(dest_path) {
        Ok(dest_meta) => {
            if src_meta.len() != dest_meta.len() {
                true
            } else if let (Ok(src_hash), Ok(dest_hash)) = (
                hash_file_quick(src_path, src_meta.len()),
                hash_file_quick(dest_path, dest_meta.len()),
            ) {
                src_hash != dest_hash
            } else {
                let src_time = src_meta.modified().ok();
                let dest_time = dest_meta.modified().ok();
                match (src_time, dest_time) {
                    (Some(st), Some(dt)) => st > dt,
                    _ => false,
                }
            }
        }
        Err(_) => true,
    };

    if should_copy {
        if let Some(parent) = dest_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match fs::copy(src_path, dest_path) {
            Ok(_) => {
                stats.copied += 1;
                stats.bytes += src_meta.len();
            }
            Err(_) => stats.errors += 1,
        }
    } else {
        stats.skipped += 1;
    }
}

fn worker_sync_walk(dir: &Path, ctx: &WorkerCtx, stats: &mut SyncStats) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let src_path = entry.path();

        if dir == ctx.home_dir && !ctx.show_all && !name.starts_with('.') {
            continue;
        }

        if src_path == ctx.sync_dir || src_path.starts_with(&ctx.sync_dir) {
            continue;
        }

        if worker_is_ignored(&src_path, ctx) {
            continue;
        }

        if ctx.synced_ignores.contains(&name) {
            continue;
        }

        let rel = src_path.strip_prefix(&ctx.home_dir).unwrap_or(&src_path);
        let dest_path = ctx.sync_dir.join(rel);

        if src_path.is_dir() {
            if !dest_path.exists() {
                fs::create_dir_all(&dest_path)?;
                stats.dirs += 1;
            }
            worker_sync_walk(&src_path, ctx, stats)?;
        } else {
            worker_sync_file(&src_path, &dest_path, stats);
        }
    }
    Ok(())
}

fn worker_clean_walk(dir: &Path, ctx: &WorkerCtx, stats: &mut SyncStats) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let dest_path = entry.path();
        let name = dest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if dir == ctx.sync_dir && ctx.synced_ignores.contains(&name) {
            continue;
        }

        let rel = match dest_path.strip_prefix(&ctx.sync_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let src_path = ctx.home_dir.join(rel);

        if dest_path.is_dir() {
            worker_clean_walk(&dest_path, ctx, stats);
            if (!src_path.exists() || worker_is_ignored(&src_path, ctx))
                && fs::remove_dir_all(&dest_path).is_ok()
            {
                stats.cleaned += 1;
            }
        } else {
            if (!src_path.exists() || worker_is_ignored(&src_path, ctx))
                && fs::remove_file(&dest_path).is_ok()
            {
                stats.cleaned += 1;
            }
        }
    }
}

fn worker_deploy_file(src_path: &Path, dest_path: &Path, stats: &mut DeployStats) {
    #[cfg(unix)]
    {
        if src_path.is_symlink() {
            if let Ok(target) = fs::read_link(src_path) {
                if dest_path.is_symlink()
                    && let Ok(existing_target) = fs::read_link(dest_path)
                    && existing_target == target
                {
                    stats.skipped += 1;
                    return;
                }
                let existed = dest_path.exists() || dest_path.is_symlink();
                if existed {
                    let _ = fs::remove_file(dest_path);
                }
                match std::os::unix::fs::symlink(&target, dest_path) {
                    Ok(_) => {
                        stats.copied += 1;
                        if existed {
                            stats.overwritten += 1;
                        }
                    }
                    Err(_) => stats.errors += 1,
                }
            }
            return;
        }
    }

    let src_meta = match fs::metadata(src_path) {
        Ok(m) => m,
        Err(_) => {
            stats.errors += 1;
            return;
        }
    };

    let (should_copy, existed) = match fs::metadata(dest_path) {
        Ok(dest_meta) => {
            let existed = dest_path.exists();
            if src_meta.len() != dest_meta.len() {
                (true, existed)
            } else if let (Ok(src_hash), Ok(dest_hash)) = (
                hash_file_quick(src_path, src_meta.len()),
                hash_file_quick(dest_path, dest_meta.len()),
            ) {
                (src_hash != dest_hash, existed)
            } else {
                let src_time = src_meta.modified().ok();
                let dest_time = dest_meta.modified().ok();
                match (src_time, dest_time) {
                    (Some(st), Some(dt)) => (st > dt, existed),
                    _ => (true, existed),
                }
            }
        }
        Err(_) => (true, false),
    };

    if should_copy {
        if let Some(parent) = dest_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match fs::copy(src_path, dest_path) {
            Ok(_) => {
                stats.copied += 1;
                if existed {
                    stats.overwritten += 1;
                }
                stats.bytes += src_meta.len();
            }
            Err(_) => stats.errors += 1,
        }
    } else {
        stats.skipped += 1;
    }
}

fn worker_deploy_walk(dir: &Path, ctx: &WorkerCtx, stats: &mut DeployStats) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let src_path = entry.path();
        let name = src_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if ctx.synced_ignores.contains(&name.to_string()) {
            continue;
        }

        let rel = match src_path.strip_prefix(&ctx.sync_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let dest_path = ctx.home_dir.join(rel);

        if src_path.is_dir() {
            if !dest_path.exists() {
                fs::create_dir_all(&dest_path)?;
                stats.dirs += 1;
            }
            worker_deploy_walk(&src_path, ctx, stats)?;
        } else {
            worker_deploy_file(&src_path, &dest_path, stats);
        }
    }
    Ok(())
}


