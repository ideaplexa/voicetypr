// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    voicetypr_lib::initialize_tls();
    match voicetypr_lib::cli::maybe_run_from_env_with_context(tauri::generate_context!()) {
        Ok(true) => return,
        Ok(false) => {}
        Err(e) => {
            eprintln!("Voicetypr CLI failed: {}", e);
            std::process::exit(1);
        }
    }

    if let Err(e) = voicetypr_lib::run() {
        eprintln!("Voicetypr failed to start: {}", e);
        std::process::exit(1);
    }
}
