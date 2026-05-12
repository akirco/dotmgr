use std::{fs, io::Read, path::Path};

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

pub fn hash_file_quick(path: &Path, size: u64) -> Result<[u8; 32], std::io::Error> {
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
