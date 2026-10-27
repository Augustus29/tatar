//! Menü çubuğu / tepsi simgesi ve popover penceresi.

use std::sync::Mutex;
use std::time::{Duration, Instant};
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
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    app.manage(PopoverState::default());
    build_popover(app)?;

    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
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
    place_under_tray(&window, tray_rect);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Popover'ı tepsi simgesinin hemen altına, yatayda ortalayarak yerleştirir.
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

/// Arayüz içerik yüksekliğini bildirince popover penceresini ona göre boyutlandırır.
pub fn resize(app: &AppHandle, height: f64) {
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        let h = height.clamp(120.0, 760.0);
        let _ = window.set_size(LogicalSize::new(POPOVER_WIDTH, h));
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        let _ = window.hide();
    }
}
