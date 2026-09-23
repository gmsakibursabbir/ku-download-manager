use crate::{show_main, AppState};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "main";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show KuDownloader", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit KuDownloader", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show, &PredefinedMenuItem::separator(app)?, &pause, &resume, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("KuDownloader")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| {
            let core = app.state::<AppState>().core.clone();
            match e.id().as_ref() {
                "show" => show_main(app),
                "pause_all" => {
                    tauri::async_runtime::spawn(async move { core.pause_all().await });
                }
                "resume_all" => {
                    tauri::async_runtime::spawn(async move { core.resume_all().await });
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn human_speed(b: i64) -> String {
    let b = b as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", b / 1024.0 / 1024.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB/s", b / 1024.0)
    } else {
        format!("{b:.0} B/s")
    }
}

pub fn update_tooltip(app: &AppHandle, down: i64, up: i64, active: usize) {
    if let Some(t) = app.tray_by_id(TRAY_ID) {
        let text = if active == 0 {
            "KuDownloader".to_string()
        } else if up > 0 {
            format!("KuDownloader — {active} active\n↓ {}  ↑ {}", human_speed(down), human_speed(up))
        } else {
            format!("KuDownloader — {active} active\n↓ {}", human_speed(down))
        };
        let _ = t.set_tooltip(Some(text));
    }
}
