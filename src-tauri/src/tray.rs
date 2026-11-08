use serde_json::json;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::config;

pub const TRAY_ID: &str = "main-tray";

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    // Leave the suspended state only once the shell is actually visible again.
    if let Some(main) = app.get_webview_window("main") {
        if !main.is_minimized().unwrap_or(false) && main.is_visible().unwrap_or(true) {
            crate::windows::resume(app, &app.state::<crate::windows::WinState>());
        }
    }
    let _ = app.emit("main-shown", ());
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let cfg = config::load(app);
    let menu = Menu::new(app)?;

    menu.append(&MenuItem::with_id(
        app,
        "open_center",
        "打开控制中心",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    if crate::auth::status(app, &app.state::<crate::auth::AuthState>()).enabled {
        menu.append(&MenuItem::with_id(
            app,
            "lock",
            "锁定 ServerDesk",
            true,
            None::<&str>,
        )?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    let services: Vec<_> = {
        let mut list: Vec<_> = cfg.services.iter().filter(|s| s.enabled).collect();
        list.sort_by_key(|s| s.sort_order);
        list.into_iter().take(8).collect()
    };
    for svc in services {
        menu.append(&MenuItem::with_id(
            app,
            format!("svc:{}", svc.id),
            if svc.name.is_empty() {
                "未命名服务"
            } else {
                &svc.name
            },
            true,
            None::<&str>,
        )?)?;
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "recheck",
        "重新检测服务",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "settings",
        "设置",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?)?;
    Ok(menu)
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = app.default_window_icon().cloned();

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("服务器控制中心")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

pub fn refresh(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn handle_menu(app: &AppHandle, id: &str) {
    if let Some(service_id) = id.strip_prefix("svc:") {
        // Never open a protected service from the tray while locked.
        let locked = crate::auth::is_locked(app, &app.state::<crate::auth::AuthState>());
        show_main(app);
        if !locked {
            let _ = app.emit("nav", json!({ "service": service_id }));
        }
        return;
    }

    match id {
        "open_center" => show_main(app),
        "settings" => {
            show_main(app);
            let _ = app.emit("nav", json!({ "view": "settings" }));
        }
        "lock" => {
            crate::auth::lock(app, &app.state::<crate::auth::AuthState>());
            show_main(app);
        }
        "recheck" => {
            let _ = app.emit("recheck", ());
        }
        "quit" => app.exit(0),
        _ => {}
    }
}
