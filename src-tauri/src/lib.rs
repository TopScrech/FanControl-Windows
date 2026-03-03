mod model;
mod provider;
mod startup;

use model::{AppModel, AppSnapshot, FanControlMode};
use provider::create_default_provider;
use std::sync::Arc;
use tokio::sync::Mutex;

type SharedModel = Arc<Mutex<AppModel>>;

#[tauri::command]
async fn initialize(model: tauri::State<'_, SharedModel>) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model.initialize().await)
}

#[tauri::command]
async fn tick(model: tauri::State<'_, SharedModel>) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model.tick().await)
}

#[tauri::command]
async fn set_selected_fan(
    model: tauri::State<'_, SharedModel>,
    selected_fan_id: i32,
) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model.set_selected_fan(selected_fan_id))
}

#[tauri::command]
async fn set_manual_rpm(
    model: tauri::State<'_, SharedModel>,
    rpm: f64,
    target_mode: Option<FanControlMode>,
) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model
        .set_manual_rpm(rpm, target_mode.unwrap_or(FanControlMode::Preset))
        .await)
}

#[tauri::command]
async fn set_control_min(model: tauri::State<'_, SharedModel>) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model.set_control_min().await)
}

#[tauri::command]
async fn set_control_max(model: tauri::State<'_, SharedModel>) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model.set_control_max().await)
}

#[tauri::command]
async fn set_auto(model: tauri::State<'_, SharedModel>) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model.set_auto().await)
}

#[tauri::command]
async fn dismiss_error(model: tauri::State<'_, SharedModel>) -> Result<AppSnapshot, String> {
    let mut model = model.lock().await;
    Ok(model.dismiss_error())
}

#[tauri::command]
async fn get_debug_text(model: tauri::State<'_, SharedModel>) -> Result<String, String> {
    let model = model.lock().await;
    Ok(model.debug_text())
}

#[tauri::command]
fn get_launch_at_login_enabled() -> Result<bool, String> {
    startup::get_launch_at_login_enabled()
}

#[tauri::command]
fn set_launch_at_login_enabled(enabled: bool) -> Result<bool, String> {
    startup::set_launch_at_login_enabled(enabled)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let provider = tauri::async_runtime::block_on(create_default_provider());
    let model = tauri::async_runtime::block_on(AppModel::new(
        provider.provider,
        provider.provider_name,
        provider.initial_notice,
    ));

    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(model)))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            initialize,
            tick,
            set_selected_fan,
            set_manual_rpm,
            set_control_min,
            set_control_max,
            set_auto,
            dismiss_error,
            get_debug_text,
            get_launch_at_login_enabled,
            set_launch_at_login_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application")
}
