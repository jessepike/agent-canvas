#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[tauri::command]
fn parse_document(source: String) -> Result<Vec<vellum_core::parse::Block>, String> {
    vellum_core::parse::parse(&source).map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::<tauri::Wry>::default()
        .invoke_handler(tauri::generate_handler![parse_document])
        .run(tauri::generate_context!())
        .expect("failed to run Vellum app");
}
