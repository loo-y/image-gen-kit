mod app_paths;
mod commands;
mod db;
mod providers;
mod secrets;
mod sqlite;
mod types;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            if let Err(err) = db::init_database() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, err).into());
            }
            let _ = app.notification().request_permission();
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_app,
            commands::list_provider_profiles,
            commands::save_provider_profile,
            commands::list_generations,
            commands::get_generation,
            commands::generate_image,
            commands::start_generation,
            commands::minimize_to_tray,
            commands::quit_app,
            commands::read_image_data_url,
            commands::read_input_image_data_urls,
            commands::reveal_image,
            commands::open_image,
            commands::open_images_dir,
            commands::reveal_debug_dir,
            commands::delete_generation
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Image Gen Kit");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, TRAY_SHOW_ID, "Show Image Gen Kit", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT_ID, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("default window icon is required for tray support");

    TrayIconBuilder::with_id("main")
        .tooltip("Image Gen Kit")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => show_main_window(app),
            TRAY_QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
