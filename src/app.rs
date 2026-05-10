use crate::config;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum IgnoreStatus {
    NotIgnored,
    DirectlyIgnored,
    InheritedIgnored,
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

pub struct App {
    pub home_dir: PathBuf,
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,

    pub ignored: HashSet<String>,
    pub config: config::AppConfig,

    pub show_all: bool,
    pub show_syncable_only: bool,

    pub full_count: usize,
    pub full_tracked: usize,
    pub full_ignored: usize,

    pub status: String,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let config = config::load_config();
        let ignored_paths = config::load_ignores();
        let mut ignored: HashSet<String> = ignored_paths.into_iter().collect();

        // 默认忽略 sync_dir
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
            full_count: 0,
            full_tracked: 0,
            full_ignored: 0,
            status: String::new(),
            should_quit: false,
        };
        app.load_entries();
        app
    }

    pub fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.home_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }

    pub fn display_path(&self) -> String {
        let rel = self.relative_path(&self.current_dir);
        if rel.is_empty() {
            "~/".into()
        } else {
            format!("~/{}", rel)
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

    pub fn load_entries(&mut self) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        if let Ok(read_dir) = fs::read_dir(&self.current_dir) {
            for entry in read_dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();

                if name == "." || name == ".." {
                    continue;
                }

                if !self.show_all && self.current_dir == self.home_dir && !name.starts_with('.') {
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

                let fe = FileEntry {
                    name,
                    path,
                    is_dir,
                    is_symlink,
                    size,
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
        if self.current_dir == self.home_dir {
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

    pub fn refresh(&mut self) {
        self.load_entries();
    }

    pub fn sync(&mut self) {
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

        let mut parts = Vec::new();

        if stats.copied > 0 {
            parts.push(format!(
                "copied {} ({})",
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
            self.status = "Already up to date ✓".into();
        } else {
            self.status = parts.join(" │ ");
        }
    }

    fn sync_walk(&self, dir: &Path, stats: &mut SyncStats) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let src_path = entry.path();

            if dir == self.home_dir && !name.starts_with('.') {
                continue;
            }

            // 跳过 sync 目录自身
            if src_path == self.config.sync_dir || src_path.starts_with(&self.config.sync_dir) {
                continue;
            }

            // 被忽略则跳过
            if self.is_ignored(&src_path) {
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
                #[cfg(unix)]
                {
                    if src_path.is_symlink() {
                        let target = fs::read_link(&src_path)?;

                        if dest_path.is_symlink()
                            && let Ok(existing_target) = fs::read_link(&dest_path)
                            && existing_target == target
                        {
                            stats.skipped += 1;
                            continue;
                        }

                        if dest_path.exists() || dest_path.is_symlink() {
                            let _ = fs::remove_file(&dest_path);
                        }
                        match std::os::unix::fs::symlink(&target, &dest_path) {
                            Ok(_) => {
                                stats.copied += 1;
                                stats.bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                            }
                            Err(_) => stats.errors += 1,
                        }
                        continue;
                    }
                }

                // 普通文件：比较修改时间判断是否需要复制
                let src_meta = entry.metadata()?;
                let should_copy = match fs::metadata(&dest_path) {
                    Ok(dest_meta) => {
                        let src_time = src_meta.modified().ok();
                        let dest_time = dest_meta.modified().ok();
                        match (src_time, dest_time) {
                            (Some(st), Some(dt)) => st > dt || src_meta.len() != dest_meta.len(),
                            _ => true,
                        }
                    }
                    Err(_) => true, // 目标不存在，需要复制
                };

                if should_copy {
                    match fs::copy(&src_path, &dest_path) {
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

                if !src_path.exists() {
                    if fs::remove_dir_all(&dest_path).is_ok() {
                        stats.cleaned += 1;
                    }
                } else if self.is_ignored(&src_path) && fs::remove_dir_all(&dest_path).is_ok() {
                    stats.cleaned += 1;
                }
            } else {
                if !src_path.exists() {
                    if fs::remove_file(&dest_path).is_ok() {
                        stats.cleaned += 1;
                    }
                } else if self.is_ignored(&src_path) && fs::remove_file(&dest_path).is_ok() {
                    stats.cleaned += 1;
                }
            }
        }
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
