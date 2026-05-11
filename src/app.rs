use crate::config;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mirror_exists: bool,
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

#[derive(Default)]
struct SyncStats {
    copied: u64,
    skipped: u64,
    cleaned: u64,
    dirs: u64,
    bytes: u64,
    errors: u64,
}

#[derive(Default)]
struct DeployStats {
    copied: u64,
    skipped: u64,
    dirs: u64,
    bytes: u64,
    errors: u64,
    overwritten: u64,
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
        };
        app.load_entries();
        app
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

                let fe = FileEntry {
                    name,
                    path,
                    is_dir,
                    is_symlink,
                    size,
                    mirror_exists,
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

        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected < self.entries.len().saturating_sub(1) {
            self.selected += 1;
        }
    }

    pub fn enter_directory(&mut self) {
        if let Some(entry) = self.entries.get(self.selected)
            && entry.is_dir
        {
            self.current_dir = entry.path.clone();
            self.selected = 0;
            self.load_entries();
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
        if let Some(entry) = self.entries.get(self.selected).cloned() {
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

            let mut stats = SyncStats::default();

            if src_path.is_dir() {
                let _ = fs::create_dir_all(&dest_path);
                stats.dirs += 1;
                if let Err(e) = self.sync_walk(&src_path, &mut stats) {
                    self.status = format!("Sync FAILED: {}", e);
                    return;
                }
                self.clean_walk(&dest_path, &mut stats);
            } else {
                self.sync_file(&src_path, &dest_path, &mut stats);
            }

            self.load_entries();
            self.status = self.format_sync_status(&stats);
        }
    }

    pub fn sync_all(&mut self) {
        let sync_dir = self.config.sync_dir.clone();

        if let Err(e) = fs::create_dir_all(&sync_dir) {
            self.status = format!("Error: {}", e);
            return;
        }

        let mut stats = SyncStats::default();

        if let Err(e) = self.sync_walk(&self.home_dir, &mut stats) {
            self.status = format!("Sync FAILED: {}", e);
            return;
        }

        self.clean_walk(&sync_dir, &mut stats);
        self.load_entries();
        self.status = self.format_sync_status(&stats);
    }

    fn format_sync_status(&self, stats: &SyncStats) -> String {
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

    fn sync_file(&self, src_path: &Path, dest_path: &Path, stats: &mut SyncStats) {
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

    fn sync_walk(&self, dir: &Path, stats: &mut SyncStats) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let src_path = entry.path();

            if dir == self.home_dir && !self.show_all && !name.starts_with('.') {
                continue;
            }

            if src_path == self.config.sync_dir || src_path.starts_with(&self.config.sync_dir) {
                continue;
            }

            if self.is_ignored(&src_path) {
                continue;
            }

            if self.config.synced_ignores.contains(&name) {
                continue;
            }

            let rel = src_path.strip_prefix(&self.home_dir).unwrap_or(&src_path);
            let dest_path = self.config.sync_dir.join(rel);

            if src_path.is_dir() {
                if !dest_path.exists() {
                    fs::create_dir_all(&dest_path)?;
                    stats.dirs += 1;
                }
                self.sync_walk(&src_path, stats)?;
            } else {
                self.sync_file(&src_path, &dest_path, stats);
            }
        }
        Ok(())
    }

    fn clean_walk(&self, dir: &Path, stats: &mut SyncStats) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let dest_path = entry.path();

            let rel = match dest_path.strip_prefix(&self.config.sync_dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let src_path = self.home_dir.join(rel);

            if dest_path.is_dir() {
                self.clean_walk(&dest_path, stats);
                if (!src_path.exists() || self.is_ignored(&src_path))
                    && fs::remove_dir_all(&dest_path).is_ok()
                {
                    stats.cleaned += 1;
                }
            } else {
                if (!src_path.exists() || self.is_ignored(&src_path))
                    && fs::remove_file(&dest_path).is_ok()
                {
                    stats.cleaned += 1;
                }
            }
        }
    }

    pub fn deploy_selected(&mut self) {
        if let Some(entry) = self.entries.get(self.selected).cloned() {
            let rel = self.relative_path(&entry.path);
            let src_path = self.config.sync_dir.join(&rel);
            let dest_path = self.home_dir.join(&rel);

            if !src_path.exists() {
                self.status = format!("Not in sync repo: {}", rel);
                return;
            }

            let mut stats = DeployStats::default();

            if src_path.is_dir() {
                let _ = fs::create_dir_all(&dest_path);
                stats.dirs += 1;
                if let Err(e) = self.deploy_walk(&src_path, &mut stats) {
                    self.status = format!("Deploy FAILED: {}", e);
                    return;
                }
            } else {
                self.deploy_file(&src_path, &dest_path, &mut stats);
            }

            self.load_entries();
            self.status = self.format_deploy_status(&stats);
        }
    }

    pub fn deploy_all(&mut self) {
        let sync_dir = self.config.sync_dir.clone();

        if !sync_dir.exists() {
            self.status = "Nothing to deploy: sync dir does not exist".into();
            return;
        }

        let mut stats = DeployStats::default();

        if let Err(e) = self.deploy_walk(&sync_dir, &mut stats) {
            self.status = format!("Deploy FAILED: {}", e);
            return;
        }

        self.load_entries();
        self.status = self.format_deploy_status(&stats);
    }

    fn format_deploy_status(&self, stats: &DeployStats) -> String {
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

    fn deploy_file(&self, src_path: &Path, dest_path: &Path, stats: &mut DeployStats) {
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

    fn deploy_walk(&self, dir: &Path, stats: &mut DeployStats) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let src_path = entry.path();
            let name = src_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if self.config.synced_ignores.contains(&name.to_string()) {
                continue;
            }

            let rel = match src_path.strip_prefix(&self.config.sync_dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let dest_path = self.home_dir.join(rel);

            if src_path.is_dir() {
                if !dest_path.exists() {
                    fs::create_dir_all(&dest_path)?;
                    stats.dirs += 1;
                }
                self.deploy_walk(&src_path, stats)?;
            } else {
                self.deploy_file(&src_path, &dest_path, stats);
            }
        }
        Ok(())
    }
}

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

fn hash_file_quick(path: &Path, size: u64) -> Result<[u8; 32], std::io::Error> {
    use sha2::{Digest, Sha256};
    use std::io::{Seek, SeekFrom};

    if size <= 1024 * 1024 {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        Ok(hasher.finalize().into())
    } else {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        let bytes_read = file.read(&mut buffer)?;
        hasher.update(&buffer[..bytes_read]);

        if let Ok(metadata) = file.metadata() {
            let file_size = metadata.len();
            if file_size > 1024 * 1024 {
                file.seek(SeekFrom::End(-(1024 * 1024) as i64))?;
                let mut end_buffer = vec![0u8; 1024 * 1024];
                let bytes_read = file.read(&mut end_buffer)?;
                hasher.update(&end_buffer[..bytes_read]);
            }
        }

        Ok(hasher.finalize().into())
    }
}
