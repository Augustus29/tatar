//! Genel (sistem çapında) klavye kısayolları.
//!
//! Arayüz kısayolu `["⇧","⌘","C"]` biçiminde saklar. macOS'ta ⌘ = Command, Windows'ta ⌘ = Ctrl,
//! ⌥ = Alt (CLAUDE.md §5). Kısayol kimlikleri: sendClip, sendFile, findDev, openWin, showHist.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::manager::ManagerHandle;
use crate::plugins::clipboard::ClipboardPlugin;

/// Kayıtlı kısayollar: kimlik -> kısayol.
#[derive(Default)]
pub struct Registered(Mutex<BTreeMap<String, Shortcut>>);

/// `["⇧","⌘","C"]` -> eklentinin anlayacağı `"Shift+Command+C"` (Windows: `"Shift+Ctrl+C"`).
/// En az bir değiştirici ve tam olarak bir asıl tuş gerekir; aksi halde `None`.
pub fn to_accelerator(keys: &[String], mac: bool) -> Option<String> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut cmd = false;
    let mut key: Option<String> = None;
    for k in keys {
        match k.as_str() {
            "⌃" => ctrl = true,
            "⌥" => alt = true,
            "⇧" => shift = true,
            "⌘" => cmd = true,
            other => {
                if key.is_some() {
                    return None;
                }
                key = Some(key_name(other)?);
            }
        }
    }
    let key = key?;
    let mut parts: Vec<&str> = Vec::new();
    // Windows'ta ⌘ de Ctrl demek; ikisi birden varsa tek Ctrl.
    let use_ctrl = ctrl || (cmd && !mac);
    if use_ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    if shift {
        parts.push("Shift");
    }
    if cmd && mac {
        parts.push("Command");
    }
    if parts.is_empty() {
        return None;
    }
    let mut out = parts.join("+");
    out.push('+');
    out.push_str(&key);
    Some(out)
}

fn key_name(k: &str) -> Option<String> {
    let named = match k {
        "↩" | "⏎" => "Enter",
        "⎋" => "Escape",
        "⇥" => "Tab",
        "␣" | " " => "Space",
        "⌫" => "Backspace",
        "←" => "ArrowLeft",
        "→" => "ArrowRight",
        "↑" => "ArrowUp",
        "↓" => "ArrowDown",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.to_string());
    }
    let mut chars = k.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        // Tek karakter: harf ve rakam büyük harfle, noktalama olduğu gibi.
        return (first.is_ascii_alphanumeric() || first.is_ascii_punctuation())
            .then(|| first.to_ascii_uppercase().to_string());
    }
    // F1..F24
    let is_f = k.len() <= 3
        && k.starts_with('F')
        && k[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n));
    is_f.then(|| k.to_string())
}

fn accelerator_for(keys: &[String]) -> Option<String> {
    to_accelerator(keys, cfg!(target_os = "macos"))
}

/// Kısayol tetiklenince yapılacak iş.
fn run_action(app: &AppHandle, id: &str) {
    match id {
        "sendClip" => {
            if let Some(clip) = app.try_state::<Arc<ClipboardPlugin>>()
                && let Err(e) = clip.send_to_first()
            {
                log::debug!("kısayolla pano gönderilemedi: {e}");
            }
        }
        "openWin" => crate::show_main(app),
        "showHist" => crate::popover::show_from_tray(app),
        // Dosya seçici ve "telefonu çaldır" arayüzde çalışır; yalnızca ana pencereye bildir.
        "sendFile" | "findDev" => {
            if id == "sendFile" {
                crate::show_main(app);
            }
            let _ = app.emit_to("main", "shortcut", serde_json::json!({ "id": id }));
        }
        _ => {}
    }
}

fn register(app: &AppHandle, id: &str, accel: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = accel.parse().map_err(|e| format!("{e}"))?;
    register_shortcut(app, id, shortcut)
}

/// Açılışta ayarlardaki kısayolları kaydeder; kaydedilemeyenler loglanır ve atlanır.
pub fn register_all(app: &AppHandle, shortcuts: &BTreeMap<String, Vec<String>>) {
    let reg = Registered::default();
    {
        let mut map = reg.0.lock().unwrap();
        for (id, keys) in shortcuts {
            let Some(accel) = accelerator_for(keys) else {
                log::warn!("kısayol çözümlenemedi: {id}");
                continue;
            };
            match register(app, id, &accel) {
                Ok(sc) => {
                    map.insert(id.clone(), sc);
                }
                Err(e) => log::warn!("kısayol kaydedilemedi ({id}): {e}"),
            }
        }
    }
    app.manage(reg);
}

/// Kısayolu değiştirir: önce yenisini dener, olmazsa eskisini korur. Kaydedilemezse `false`.
#[tauri::command]
pub fn set_shortcut(
    app: AppHandle,
    mgr: State<'_, ManagerHandle>,
    reg: State<'_, Registered>,
    id: String,
    keys: Vec<String>,
) -> bool {
    let store = mgr.settings_store();
    if !store.get().shortcuts.contains_key(&id) {
        return false;
    }
    let Some(accel) = accelerator_for(&keys) else {
        return false;
    };
    let mut map = reg.0.lock().unwrap();
    let old = map.remove(&id);
    if let Some(old) = &old {
        let _ = app.global_shortcut().unregister(*old);
    }
    match register(&app, &id, &accel) {
        Ok(sc) => {
            map.insert(id.clone(), sc);
            drop(map);
            if let Some(s) = store.set_shortcut(&id, keys) {
                let _ = app.emit("settings-changed", &s);
            }
            true
        }
        Err(e) => {
            log::warn!("kısayol kaydedilemedi ({id}): {e}");
            // Eskisini geri yükle.
            if let Some(old) = old
                && let Ok(sc) = register_shortcut(&app, &id, old)
            {
                map.insert(id.clone(), sc);
            }
            false
        }
    }
}

fn register_shortcut(app: &AppHandle, id: &str, shortcut: Shortcut) -> Result<Shortcut, String> {
    let id_owned = id.to_string();
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                run_action(app, &id_owned);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mac_accelerators() {
        assert_eq!(
            to_accelerator(&k(&["⇧", "⌘", "C"]), true).unwrap(),
            "Shift+Command+C"
        );
        assert_eq!(
            to_accelerator(&k(&["⌥", "⌘", "R"]), true).unwrap(),
            "Alt+Command+R"
        );
        assert_eq!(
            to_accelerator(&k(&["⌃", "⌥", "⇧", "⌘", "v"]), true).unwrap(),
            "Ctrl+Alt+Shift+Command+V"
        );
    }

    #[test]
    fn windows_maps_cmd_to_ctrl() {
        assert_eq!(
            to_accelerator(&k(&["⇧", "⌘", "C"]), false).unwrap(),
            "Ctrl+Shift+C"
        );
        assert_eq!(
            to_accelerator(&k(&["⌥", "⌘", "T"]), false).unwrap(),
            "Ctrl+Alt+T"
        );
        // ⌃ ve ⌘ birlikte tek Ctrl olur.
        assert_eq!(
            to_accelerator(&k(&["⌃", "⌘", "T"]), false).unwrap(),
            "Ctrl+T"
        );
    }

    #[test]
    fn rejects_invalid() {
        assert!(to_accelerator(&k(&["C"]), true).is_none()); // değiştirici yok
        assert!(to_accelerator(&k(&["⇧", "⌘"]), true).is_none()); // asıl tuş yok
        assert!(to_accelerator(&k(&["⌘", "A", "B"]), true).is_none());
        assert!(to_accelerator(&k(&["⌘", "Foo"]), true).is_none());
        assert_eq!(
            to_accelerator(&k(&["⌘", "F5"]), true).unwrap(),
            "Command+F5"
        );
    }
}
