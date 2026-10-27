//! Kalıcı veri: güvenilir cihazlar ve ayarlar (uygulama veri klasöründe JSON).

mod settings;
mod trusted;

pub use settings::{Settings, SettingsPatch, SettingsStore};
pub use trusted::{TrustedDevice, TrustedStore};

use std::path::Path;

/// Yarım yazılmış dosya kalmasın diye geçici dosya + yeniden adlandırma. Unix'te yalnızca sahibi okur.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}
