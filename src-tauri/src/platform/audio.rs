//! Sistem varsayılan çıkış aygıtının sesini okuma / ayarlama. Küçük ve platformdan bağımsız API;
//! gerçek iş macOS'ta CoreAudio, Windows'ta `IAudioEndpointVolume` ile yapılır.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputState {
    pub muted: bool,
    /// 0.0 - 1.0
    pub volume: f32,
}

/// Ses arka ucu. Telefon eklentisi bunun üzerinden çalışır; testlerde sahtesi kullanılır.
pub trait AudioBackend: Send + Sync {
    /// Mevcut durum; çıkış aygıtı yok / ses denetimi desteklenmiyorsa `None`.
    fn get(&self) -> Option<OutputState>;
    /// Durumu uygular; başarılıysa `true`.
    fn set(&self, state: OutputState) -> bool;
}

/// İşletim sisteminin gerçek ses arka ucu.
pub struct SystemAudio;

impl AudioBackend for SystemAudio {
    fn get(&self) -> Option<OutputState> {
        imp::get_output_state()
    }
    fn set(&self, state: OutputState) -> bool {
        imp::set_output_state(state)
    }
}

#[cfg(target_os = "macos")]
#[path = "audio_macos.rs"]
mod imp;

#[cfg(windows)]
#[path = "audio_windows.rs"]
mod imp;

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use super::OutputState;
    pub fn get_output_state() -> Option<OutputState> {
        None
    }
    pub fn set_output_state(_state: OutputState) -> bool {
        false
    }
}
