//! Menü çubuğu / tepsi simgesi ve popover penceresi.

use std::sync::Mutex;
use std::time::{Duration, Instant};
#[cfg(windows)]
use tauri::menu::{Menu, MenuItem};
#[cfg(target_os = "macos")]
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub const POPOVER_LABEL: &str = "popover";
const POPOVER_WIDTH: f64 = 340.0;
const POPOVER_INITIAL_HEIGHT: f64 = 520.0;
const POPOVER_GAP: f64 = 6.0;

/// Popover en son ne zaman odağı kaybedip gizlendi? Tepsi tıklaması
/// aynı anda geldiğinde popover'ı hemen yeniden açmamak için.
#[derive(Default)]
pub struct PopoverState {
    last_blur_hide: Mutex<Option<Instant>>,
    /// Son tepsi simgesi konumu (Windows'ta içerik yüksekliği değişince yeniden hizalamak için).
    last_tray_rect: Mutex<Option<Rect>>,
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    app.manage(PopoverState::default());
    build_popover(app)?;

    // macOS: siyah "template" siluet (sistem renklendirir). Windows'ta şablon yok; görev çubuğu
    // koyu da olsa açık da olsa görünsün diye renkli uygulama simgesi kullanılır.
    #[cfg(not(windows))]
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    #[cfg(windows)]
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Tatar")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle(tray.app_handle(), rect);
            }
        })
        .build(app)?;
    Ok(())
}

fn build_popover(app: &AppHandle) -> tauri::Result<()> {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut builder =
        WebviewWindowBuilder::new(app, POPOVER_LABEL, WebviewUrl::App("index.html".into()))
            .title("Tatar")
            .inner_size(POPOVER_WIDTH, POPOVER_INITIAL_HEIGHT)
            .decorations(false)
            .transparent(true)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .shadow(true);

    #[cfg(target_os = "macos")]
    {
        builder = builder.effects(
            EffectsBuilder::new()
                .effect(Effect::Popover)
                .state(EffectState::Active)
                .radius(22.0)
                .build(),
        );
    }

    let window = builder.build()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event
            && let Some(w) = handle.get_webview_window(POPOVER_LABEL)
            && w.is_visible().unwrap_or(false)
        {
            let _ = w.hide();
            if let Some(state) = handle.try_state::<PopoverState>() {
                *state.last_blur_hide.lock().unwrap() = Some(Instant::now());
            }
        }
    });
    Ok(())
}

fn toggle(app: &AppHandle, tray_rect: Rect) {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    if let Some(state) = app.try_state::<PopoverState>() {
        let recent = state
            .last_blur_hide
            .lock()
            .unwrap()
            .is_some_and(|t| t.elapsed() < Duration::from_millis(250));
        if recent {
            return;
        }
    }
    place(&window, tray_rect);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Platforma göre yerleştirme: macOS'ta simgenin altına, Windows'ta görev çubuğunun kenarına göre.
fn place(window: &WebviewWindow, tray_rect: Rect) {
    if let Some(state) = window.app_handle().try_state::<PopoverState>() {
        *state.last_tray_rect.lock().unwrap() = Some(tray_rect);
    }
    #[cfg(windows)]
    place_near_taskbar(window, tray_rect);
    #[cfg(not(windows))]
    place_under_tray(window, tray_rect);
}

/// Popover'ı tepsi simgesinin hemen altına, yatayda ortalayarak yerleştirir.
#[cfg_attr(windows, allow(dead_code))]
fn place_under_tray(window: &WebviewWindow, tray_rect: Rect) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let pos = tray_rect.position.to_physical::<f64>(scale);
    let size = tray_rect.size.to_physical::<f64>(scale);
    let win_w = window
        .outer_size()
        .map(|s| s.width as f64)
        .unwrap_or(POPOVER_WIDTH * scale);

    let mut x = pos.x + size.width / 2.0 - win_w / 2.0;
    let y = pos.y + size.height + POPOVER_GAP * scale;

    // Ekranın dışına taşmasın.
    if let Ok(Some(monitor)) = window.monitor_from_point(pos.x, pos.y) {
        let m_pos = monitor.position();
        let m_size = monitor.size();
        let min_x = m_pos.x as f64 + 8.0 * scale;
        let max_x = m_pos.x as f64 + m_size.width as f64 - win_w - 8.0 * scale;
        x = x.clamp(min_x, max_x.max(min_x));
    }
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Sağ tık menüsünü (Tatar'ı aç / Çık) kurar. Windows'ta kullanılır; dil ayarı okunabilsin diye
/// protokol yöneticisi başladıktan sonra çağrılır. macOS'ta menü çubuğu simgesi menü göstermez.
#[cfg(windows)]
pub fn set_tray_menu(app: &AppHandle, language: &str) -> tauri::Result<()> {
    let lang = crate::i18n::resolve_language(language);
    let open = MenuItem::with_id(
        app,
        "tray-open",
        crate::i18n::notify_text(lang, "trayOpen", &[]),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "tray-quit",
        crate::i18n::notify_text(lang, "trayQuit", &[]),
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
        tray.on_menu_event(|app, event| match event.id().as_ref() {
            "tray-open" => crate::show_main(app),
            "tray-quit" => app.exit(0),
            _ => {}
        });
    }
    Ok(())
}

/// Tamsayı dikdörtgen (fiziksel piksel): sol, üst, sağ, alt.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
struct Edges {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

/// Popover'ın sol-üst köşesini hesaplar. `tray`: simgenin dikdörtgeni, `win`: pencere boyutu,
/// `monitor`: ekranın tamamı, `work`: görev çubuğu hariç çalışma alanı. Görev çubuğunun hangi
/// kenarda olduğu, çalışma alanının ekrandan hangi kenarda daha çok kısıldığından anlaşılır
/// (hiçbiri kısılmamışsa, ör. otomatik gizleme, simgeye en yakın kenar alınır). Sonuç çalışma
/// alanına sığdırılır.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn taskbar_popover_position(
    tray: Edges,
    win: (f64, f64),
    monitor: Edges,
    work: Edges,
    gap: f64,
) -> (f64, f64) {
    let (w, h) = win;
    let cx = (tray.left + tray.right) / 2.0;
    let cy = (tray.top + tray.bottom) / 2.0;
    let gaps = [
        ("left", work.left - monitor.left),
        ("top", work.top - monitor.top),
        ("right", monitor.right - work.right),
        ("bottom", monitor.bottom - work.bottom),
    ];
    let mut edge = "bottom";
    let mut best = 0.5;
    for (name, g) in gaps {
        if g > best {
            best = g;
            edge = name;
        }
    }
    if best <= 0.5 {
        // Görev çubuğu çalışma alanını daraltmıyor: simgeye en yakın kenar.
        let d = [
            ("left", cx - monitor.left),
            ("top", cy - monitor.top),
            ("right", monitor.right - cx),
            ("bottom", monitor.bottom - cy),
        ];
        edge = d
            .iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or("bottom", |e| e.0);
    }
    let (x, y) = match edge {
        "top" => (cx - w / 2.0, tray.bottom + gap),
        "left" => (tray.right + gap, cy - h / 2.0),
        "right" => (tray.left - w - gap, cy - h / 2.0),
        _ => (cx - w / 2.0, tray.top - h - gap),
    };
    let min_x = work.left;
    let max_x = (work.right - w).max(min_x);
    let min_y = work.top;
    let max_y = (work.bottom - h).max(min_y);
    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
}

/// Windows: popover'ı görev çubuğunun konumuna göre simgenin yanına/üstüne yerleştirir.
#[cfg(windows)]
fn place_near_taskbar(window: &WebviewWindow, tray_rect: Rect) {
    // Tepsi olayındaki konum fiziksel pikseldir; `to_physical` yine de ölçek verir.
    let scale = window.scale_factor().unwrap_or(1.0);
    let pos = tray_rect.position.to_physical::<f64>(scale);
    let size = tray_rect.size.to_physical::<f64>(scale);
    let tray = Edges {
        left: pos.x,
        top: pos.y,
        right: pos.x + size.width,
        bottom: pos.y + size.height,
    };
    let (win_w, win_h) = window
        .outer_size()
        .map(|s| (s.width as f64, s.height as f64))
        .unwrap_or((POPOVER_WIDTH * scale, POPOVER_INITIAL_HEIGHT * scale));

    // Simgenin bulunduğu ekran (çoklu ekran ve farklı ölçekler için). Ölçeği ekrandan al.
    let center = (tray.left + size.width / 2.0, tray.top + size.height / 2.0);
    let monitor = window
        .monitor_from_point(center.0, center.1)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let m_scale = monitor.scale_factor();
    let gap = POPOVER_GAP * m_scale;
    let mp = monitor.position();
    let ms = monitor.size();
    let wa = monitor.work_area();
    let monitor_edges = Edges {
        left: mp.x as f64,
        top: mp.y as f64,
        right: mp.x as f64 + ms.width as f64,
        bottom: mp.y as f64 + ms.height as f64,
    };
    let work = Edges {
        left: wa.position.x as f64,
        top: wa.position.y as f64,
        right: wa.position.x as f64 + wa.size.width as f64,
        bottom: wa.position.y as f64 + wa.size.height as f64,
    };
    let (x, y) = taskbar_popover_position(tray, (win_w, win_h), monitor_edges, work, gap);
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Arayüz içerik yüksekliğini bildirince popover penceresini ona göre boyutlandırır.
pub fn resize(app: &AppHandle, height: f64) {
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        let h = height.clamp(120.0, 760.0);
        let _ = window.set_size(LogicalSize::new(POPOVER_WIDTH, h));
        // Windows'ta görev çubuğu altta ise pencere yukarı doğru büyümeli: yeniden hizala.
        #[cfg(windows)]
        if window.is_visible().unwrap_or(false)
            && let Some(state) = app.try_state::<PopoverState>()
            && let Some(rect) = *state.last_tray_rect.lock().unwrap()
        {
            place_near_taskbar(&window, rect);
        }
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        let _ = window.hide();
    }
}

/// Popover'ı tepsi simgesinin altında açar (kısayoldan). Simge konumu yoksa hiçbir şey yapmaz.
pub fn show_from_tray(app: &AppHandle) {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    if let Some(tray) = app.tray_by_id("main")
        && let Ok(Some(rect)) = tray.rect()
    {
        place(&window, rect);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Edges = Edges {
        left: 0.0,
        top: 0.0,
        right: 1920.0,
        bottom: 1080.0,
    };

    fn tray_at(x: f64, y: f64) -> Edges {
        Edges {
            left: x,
            top: y,
            right: x + 24.0,
            bottom: y + 24.0,
        }
    }

    #[test]
    fn bottom_taskbar_opens_above_icon() {
        let work = Edges {
            bottom: 1040.0,
            ..MON
        };
        let (x, y) =
            taskbar_popover_position(tray_at(1800.0, 1050.0), (340.0, 500.0), MON, work, 6.0);
        // Alt kenar görev çubuğunun üstünde kalır.
        assert_eq!(y, 1040.0 - 500.0);
        assert!(x + 340.0 <= work.right);
    }

    #[test]
    fn top_taskbar_opens_below_icon() {
        let work = Edges { top: 40.0, ..MON };
        let (_, y) = taskbar_popover_position(tray_at(900.0, 8.0), (340.0, 500.0), MON, work, 6.0);
        // Görev çubuğunun altına, çalışma alanının üst kenarına oturur.
        assert_eq!(y, 40.0);
    }

    #[test]
    fn side_taskbars_open_beside_icon() {
        let left = Edges { left: 60.0, ..MON };
        let (x, _) = taskbar_popover_position(tray_at(30.0, 500.0), (340.0, 500.0), MON, left, 6.0);
        assert_eq!(x, 30.0 + 24.0 + 6.0);
        let right = Edges {
            right: 1860.0,
            ..MON
        };
        let (x, _) =
            taskbar_popover_position(tray_at(1870.0, 500.0), (340.0, 500.0), MON, right, 6.0);
        assert_eq!(x, 1860.0 - 340.0);
    }

    #[test]
    fn result_is_clamped_into_work_area() {
        let work = Edges {
            bottom: 1040.0,
            ..MON
        };
        // Simge ekranın sol kenarında: pencere dışarı taşmamalı.
        let (x, y) =
            taskbar_popover_position(tray_at(0.0, 1050.0), (340.0, 2000.0), MON, work, 6.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn autohide_taskbar_uses_nearest_edge() {
        let (_, y) =
            taskbar_popover_position(tray_at(900.0, 1056.0), (340.0, 500.0), MON, MON, 6.0);
        assert_eq!(y, 1056.0 - 500.0 - 6.0);
    }
}
