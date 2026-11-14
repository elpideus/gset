//! GSet's settings window: a Tauri shell around gset-core.

mod art;
mod theme;
pub mod views;

use tauri::Manager;

pub fn start() {
    let steam_root = gset_core::steam::steam_root(&gset_core::Paths::new().home);
    tauri::Builder::default()
        .register_uri_scheme_protocol("art", move |_ctx, req| art::handle(&steam_root, &req))
        .invoke_handler(tauri::generate_handler![
            views::library,
            views::game,
            views::set_value,
            views::use_suggestions,
            views::dismiss_suggestion,
            views::pin,
            views::apply_update,
            views::undo_update,
            views::play,
            views::app_settings,
            views::set_app_option,
            views::save_preset,
            views::delete_preset,
            views::theme,
        ])
        .setup(|app| {
            theme::watch(app.handle().clone());
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("GSet's window couldn't start");
}
