//! Windows ses denetimi: varsayılan oynatma aygıtında `IAudioEndpointVolume`.

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::Result as WinResult;

use super::OutputState;

/// Her çağrıda mevcut iş parçacığı için COM'u başlatır (zaten başlatılmışsa hata yok sayılır:
/// `S_FALSE` / `RPC_E_CHANGED_MODE` durumunda da COM kullanılabilir).
fn endpoint() -> WinResult<IAudioEndpointVolume> {
    // SAFETY: COM çağrıları; dönen arayüz nesneleri `windows` tarafından yaşam süresi yönetilir.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
    }
}

pub fn get_output_state() -> Option<OutputState> {
    let ep = endpoint().ok()?;
    // SAFETY: geçerli bir IAudioEndpointVolume üzerinde salt okunur çağrılar.
    unsafe {
        let muted = ep.GetMute().ok()?.as_bool();
        let volume = ep.GetMasterVolumeLevelScalar().ok()?;
        Some(OutputState {
            muted,
            volume: volume.clamp(0.0, 1.0),
        })
    }
}

pub fn set_output_state(state: OutputState) -> bool {
    let Ok(ep) = endpoint() else {
        return false;
    };
    // SAFETY: geçerli bir IAudioEndpointVolume üzerinde çağrılar; olay bağlamı boş (null).
    unsafe {
        let a = ep.SetMasterVolumeLevelScalar(state.volume.clamp(0.0, 1.0), std::ptr::null());
        let b = ep.SetMute(state.muted, std::ptr::null());
        a.is_ok() && b.is_ok()
    }
}
