mod app_paths;
mod commands;
mod db;
mod providers;
mod secrets;
mod sqlite;
mod types;

pub fn run() {
    tauri::Builder::default()
        .setup(|_| {
            if let Err(err) = db::init_database() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, err).into());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_app,
            commands::list_provider_profiles,
            commands::save_provider_profile,
            commands::list_generations,
            commands::get_generation,
            commands::generate_image,
            commands::read_image_data_url,
            commands::reveal_image,
            commands::delete_generation
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Image Gen Kit");
}
