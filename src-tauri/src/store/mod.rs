//! Kalıcı veri: güvenilir cihazlar ve ayarlar (uygulama veri klasöründe JSON).

mod settings;
mod trusted;

pub use settings::{Settings, SettingsPatch, SettingsStore};
pub use trusted::{TrustedDevice, TrustedStore};

use std::path::Path;

/// Yarım yazılmış dosya kalmasın diye geçici dosya + fsync + yeniden adlandırma.
/// Geçici dosya aynı klasörde, rastgele adla ve (unix'te) en baştan 0o600 izniyle oluşturulur;
/// böylece içerik hiçbir an başkalarınca okunabilir olmaz. Özel anahtar benzeri veriler için de kullanılır.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{:016x}.tmp", rand::random::<u64>()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let result = (|| {
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_replaces_and_is_private() {
        let dir = std::env::temp_dir().join(format!("tatar-wa-{:016x}", rand::random::<u64>()));
        let f = dir.join("a.json");
        write_atomic(&f, b"bir").unwrap();
        write_atomic(&f, b"iki").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"iki");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&f).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // Geçici dosya artığı kalmaz.
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(left.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
