//! Sorun giderme için platforma özel ağ bilgileri (engelleyen işler: `spawn_blocking` içinde çağır)
//! ve sistem ayarları sayfalarını açma.

use std::process::Command;

use crate::protocol::discovery::local_ipv4_interfaces;
use crate::troubleshoot::{Firewall, LocalNet, WifiFacts};

/// Çalıştırılabilir dosyanın yolu (güvenlik duvarı kuralı eşleştirmesi için).
fn current_exe_string() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Windows sistem aracının tam yolu (`%SystemRoot%\System32\<ad>.exe`).
/// PATH üzerinden aranmaz; geçerli dizine ya da PATH'e yerleştirilmiş sahte bir program çalışmasın diye.
fn system32_exe(name: &str) -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    format!(r"{root}\System32\{name}.exe")
}

/// Windows'ta konsol penceresi açmadan alt süreç çalıştırır.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Arayüzlerden Wi‑Fi/LAN olgularını ve (varsa) ağ adını toplar.
pub fn wifi_facts() -> WifiFacts {
    let addrs: Vec<_> = local_ipv4_interfaces()
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    WifiFacts::from_addrs(&addrs, ssid())
}

#[cfg(target_os = "macos")]
fn ssid() -> Option<String> {
    // Yeni macOS sürümleri ağ adını konum izni olmadan gizleyebilir; bu durumda None döner ve
    // arayüz adsız metni gösterir.
    ["en0", "en1"].iter().find_map(|dev| {
        crate::troubleshoot::parse_networksetup_ssid(&run(
            "/usr/sbin/networksetup",
            &["-getairportnetwork", dev],
        )?)
    })
}

#[cfg(windows)]
fn ssid() -> Option<String> {
    crate::troubleshoot::parse_netsh_ssid(&run(
        &system32_exe("netsh"),
        &["wlan", "show", "interfaces"],
    )?)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn ssid() -> Option<String> {
    None
}

/// macOS'ta yerel ağ izni doğrudan sorgulanamaz → "belirsiz"; diğer platformlarda gerekmez.
pub fn local_network_default() -> LocalNet {
    if cfg!(target_os = "macos") {
        LocalNet::Unknown
    } else {
        LocalNet::NotApplicable
    }
}

/// Güvenlik duvarı durumu. macOS: Uygulama Güvenlik Duvarı (`socketfilterfw`); Windows: `netsh`.
#[cfg(target_os = "macos")]
pub fn firewall_state() -> Firewall {
    const ALF: &str = "/usr/libexec/ApplicationFirewall/socketfilterfw";
    let Some(on) = run(ALF, &["--getglobalstate"])
        .as_deref()
        .and_then(crate::troubleshoot::parse_alf_global_state)
    else {
        return Firewall::Unknown;
    };
    if !on {
        return Firewall::Open;
    }
    let exe = current_exe_string();
    match run(ALF, &["--listapps"])
        .as_deref()
        .and_then(|o| crate::troubleshoot::parse_alf_listapps(o, &exe))
    {
        Some(true) => Firewall::Allowed,
        Some(false) => Firewall::Blocked,
        None => Firewall::Unknown,
    }
}

#[cfg(windows)]
pub fn firewall_state() -> Firewall {
    let exe = current_exe_string();
    let out = run(
        &system32_exe("netsh"),
        &[
            "advfirewall",
            "firewall",
            "show",
            "rule",
            "name=all",
            "dir=in",
            "verbose",
        ],
    );
    match out
        .as_deref()
        .and_then(|o| crate::troubleshoot::parse_netsh_rules(o, &exe))
    {
        Some(true) => Firewall::Allowed,
        Some(false) => Firewall::Blocked,
        None => Firewall::Unknown,
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn firewall_state() -> Firewall {
    let _ = current_exe_string;
    Firewall::Open
}

/// Sistem ayarları hedefine karşılık gelen eylem.
#[derive(Debug, PartialEq, Eq)]
pub enum SettingsAction {
    Url(&'static str),
    Command(&'static str, &'static [&'static str]),
    Nothing,
}

pub fn settings_action(target: &str, mac: bool, windows: bool) -> SettingsAction {
    match (target, mac, windows) {
        ("localNetwork", true, _) => SettingsAction::Url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork",
        ),
        ("firewall", true, _) => {
            SettingsAction::Url("x-apple.systempreferences:com.apple.preference.security?Firewall")
        }
        ("firewall", _, true) => SettingsAction::Command("control", &["firewall.cpl"]),
        _ => SettingsAction::Nothing,
    }
}

pub fn open_system_settings(target: &str) -> Result<(), String> {
    match settings_action(target, cfg!(target_os = "macos"), cfg!(windows)) {
        SettingsAction::Url(u) => {
            tauri_plugin_opener::open_url(u, None::<&str>).map_err(|e| e.to_string())
        }
        // Komut adı (ör. "control") yalnızca Windows'ta üretilir; tam yola çevrilip çalıştırılır.
        SettingsAction::Command(p, a) => Command::new(system32_exe(p))
            .args(a)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        SettingsAction::Nothing => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_targets_map_per_platform() {
        assert!(matches!(
            settings_action("localNetwork", true, false),
            SettingsAction::Url(u) if u.ends_with("Privacy_LocalNetwork")
        ));
        assert!(matches!(
            settings_action("firewall", true, false),
            SettingsAction::Url(u) if u.ends_with("?Firewall")
        ));
        assert_eq!(
            settings_action("firewall", false, true),
            SettingsAction::Command("control", &["firewall.cpl"])
        );
        assert_eq!(
            settings_action("localNetwork", false, true),
            SettingsAction::Nothing
        );
        assert_eq!(
            settings_action("bogus", true, false),
            SettingsAction::Nothing
        );
    }
}
