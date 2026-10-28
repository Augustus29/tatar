//! Windows'a özel kodlar: "Saydamlık efektleri" ayarı ve Mica pencere zemini.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::window::{Effect, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue,
    RegOpenKeyExW,
};
use windows::core::w;

/// Mica'nın desteklendiği ilk Windows 11 derleme numarası.
const MICA_MIN_BUILD: u32 = 22000;

static MICA_ACTIVE: AtomicBool = AtomicBool::new(false);
static POPOVER_ACRYLIC: AtomicBool = AtomicBool::new(false);

/// `HKCU\...\Personalize\EnableTransparency` değeri (yoksa `None`).
fn read_enable_transparency() -> Option<u32> {
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `data` ve `size` çağrı boyunca geçerli; boyut 4 bayt DWORD için doğru.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (status.0 == 0).then_some(data)
}

/// Sistemde "Saydamlık efektleri" kapalı mı? (Değer yoksa açık sayılır.)
pub fn reduce_transparency() -> bool {
    matches!(read_enable_transparency(), Some(0))
}

/// Ayar değişince arayüze `reduce-transparency-changed` olayı gönderir.
/// Kayıt defteri anahtarını engelleyen bir iş parçacığında bekler; boştayken işlemci kullanmaz.
pub fn watch_reduce_transparency(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut key = HKEY(std::ptr::null_mut());
        // SAFETY: `key` geçerli bir çıktı işaretçisi; açılan tutamaç aşağıda kapatılır.
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                None,
                KEY_NOTIFY | KEY_READ,
                &mut key,
            )
        };
        if opened.0 != 0 {
            return;
        }
        let mut last = reduce_transparency();
        loop {
            // SAFETY: `key` açık bir anahtar; eşzamanlı (engelleyen) bekleme.
            let status = unsafe {
                RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, None, false)
            };
            if status.0 != 0 {
                break;
            }
            let now = reduce_transparency();
            if now != last {
                last = now;
                let _ = app.emit("reduce-transparency-changed", now);
            }
        }
        // SAFETY: yukarıda açılan tutamaç.
        let _ = unsafe { RegCloseKey(key) };
    });
}

/// Windows derleme numarası (`CurrentBuildNumber`).
fn windows_build() -> Option<u32> {
    let mut buf = [0u16; 16];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf` ve `size` (bayt cinsinden) tutarlı.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if status.0 != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len]).trim().parse().ok()
}

/// Windows 11'de ana pencereye Mica zemini verir. Windows 10'da (ya da hata olursa) hiçbir şey
/// yapmaz; arayüz opak zeminde kalır. Mica uygulandıysa `true`.
pub fn apply_window_material(app: &AppHandle) -> bool {
    let supported = windows_build().is_some_and(|b| b >= MICA_MIN_BUILD);
    let ok = supported
        && app.get_webview_window("main").is_some_and(|w| {
            w.set_effects(EffectsBuilder::new().effect(Effect::Mica).build())
                .is_ok()
        });
    MICA_ACTIVE.store(ok, Ordering::Relaxed);
    apply_popover_acrylic(app, supported);
    ok
}

/// Popover'a Acrylic (arkasını bulanıklaştıran) zemin verir: saydam pencerede `backdrop-filter`
/// yalnızca pencerenin kendi içeriğini bulandırdığından masaüstü bulanmadan görünüyordu.
/// Yalnızca Windows 11'de; olmazsa popover eskisi gibi kalır.
fn apply_popover_acrylic(app: &AppHandle, supported: bool) {
    let ok = supported
        && app
            .get_webview_window(crate::popover::POPOVER_LABEL)
            .is_some_and(|w| {
                w.set_effects(
                    EffectsBuilder::new()
                        .effect(Effect::Acrylic)
                        .color(tauri::window::Color(128, 128, 128, 24))
                        .build(),
                )
                .is_ok()
            });
    POPOVER_ACRYLIC.store(ok, Ordering::Relaxed);
}

pub fn popover_acrylic_active() -> bool {
    POPOVER_ACRYLIC.load(Ordering::Relaxed)
}

pub fn window_material_active() -> bool {
    MICA_ACTIVE.load(Ordering::Relaxed)
}
