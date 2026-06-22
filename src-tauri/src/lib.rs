pub mod character_dict;
pub mod commands;
pub mod dictionary;
pub mod envstore;
pub mod llm;
pub mod log;
pub mod merge;
pub mod project;
pub mod scraper;
pub mod srt;
pub mod translation;
pub mod web_search;

use commands::project::AppState;
use envstore::EnvStoreState;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Load persisted env vars into process env, and seed the in-memory store.
            let store = envstore::load_into_process(&app.handle().clone());
            app.manage(EnvStoreState(std::sync::Mutex::new(store)));

            // Restore active env var name from settings.json
            let app_state = AppState::default();
            let dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let settings_path = dir.join("settings.json");
            if settings_path.exists() {
                if let Ok(data) = std::fs::read_to_string(&settings_path) {
                    if let Ok(settings) = serde_json::from_str::<serde_json::Value>(&data) {
                        if let Some(name) = settings["active_env_var"].as_str() {
                            if !name.is_empty() {
                                let mut active = app_state
                                    .active_env_var
                                    .lock()
                                    .unwrap();
                                *active = Some(name.to_string());
                            }
                        }
                    }
                }
            }
            app.manage(app_state);

            // MDL WebView extraction state
            app.manage(scraper::mydramalist::MdlExtractState(std::sync::Mutex::new(None)));
            app.manage(scraper::mydramalist::MdlPageInfoState(std::sync::Mutex::new(None)));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::project::create_project,
            commands::project::open_project,
            commands::project::get_project_config,
            commands::project::save_project_config,
            commands::srt::parse_srt_file,
            commands::srt::get_srt_entries,
            commands::srt::save_srt_file,
            commands::srt::list_srt_in_dir,
            commands::srt::generate_srt_synopsis,
            commands::srt::detect_srt_scenes,
            commands::srt::analyze_scene_context,
            commands::srt::save_srt_analysis,
            commands::srt::load_srt_analyses,
            commands::srt::resolve_synopsis_katakana,
            commands::srt::resolve_unresolved_term_ai,
            commands::srt::resolve_unresolved_term_ai_openai,
            commands::srt::resolve_unresolved_terms_batch_openai,
            commands::dictionary::load_character_dictionary,
            commands::dictionary::get_characters,
            commands::dictionary::save_character_dictionary,
            commands::dictionary::load_glossary_dictionary,
            commands::dictionary::get_glossary,
            commands::dictionary::save_glossary_dictionary,
            commands::llm::set_provider_config,
            commands::llm::get_provider_config,
            commands::llm::test_llm_connection,
            commands::llm::check_active_connection,
            commands::llm::set_active_env_var,
            commands::llm::get_active_env_var,
            commands::llm::check_env_var_key_exists,
            commands::llm::list_provider_presets,
            envstore::get_env_var,
            envstore::set_env_var,
            envstore::delete_env_var,
            envstore::list_env_vars,
            commands::translation::start_translation,
            commands::translation::cancel_translation,
            commands::scraper::scrape_url,
            commands::scraper::scrape_all,
            commands::scraper::merge_characters,
            commands::scraper::save_scrape_result,
            commands::scraper::load_scrape_result,
            commands::scraper::save_merged_characters,
            commands::scraper::load_merged_characters,
            commands::scraper::merged_to_dictionary,
            commands::scraper::save_raw_html,
            commands::scraper::parse_mdl_paste,
            commands::scraper::parse_mdl_html_paste,
            commands::scraper::parse_douban_paste,
            commands::scraper::parse_tmdb_paste,
            commands::scraper::search_tmdb,
            commands::scraper::scrape_tmdb_credits,
            commands::scraper::build_character_dict,
            commands::scraper::enrich_dict_kanji,
            commands::scraper::merge_cast_entries,
            commands::scraper::verify_character_dict,
            commands::scraper::generate_character_aliases,
            commands::scraper::search_database_url,
            commands::scraper::open_mdl_window,
            commands::scraper::receive_mdl_extract,
            commands::scraper::get_mdl_extract,
            commands::scraper::extract_mdl_data,
            commands::scraper::inspect_mdl_page,
            commands::scraper::receive_mdl_page_info,
            commands::scraper::get_mdl_page_info,
            commands::scraper::close_mdl_window,
            commands::drama_info::save_drama_info,
            commands::drama_info::load_drama_info,
            commands::service_settings::get_service_settings,
            commands::service_settings::save_service_settings,
            commands::service_settings::test_tmdb_connection,
            commands::service_settings::test_openai_ai_confirm,
            commands::service_settings::get_provider_settings,
            commands::service_settings::save_provider_settings,
            commands::util::open_url,
            commands::synopsis::summarize_synopsis,
            commands::ja_kanji::correct_ja_kanji,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
