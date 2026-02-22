// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri_plugin_shell::ShellExt;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn execute_engine_command(command: String) -> Result<String, String> {
    spatia_engine::execute_command(&command).map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let sidecar = app.shell().sidecar("spatia-geocoder")?;
            sidecar.spawn()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, execute_engine_command])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
