const CONSOLE_NOTICE: &str = "NE FERMEZ PAS CETTE FENÊTRE SINON TOUTE L'APP COQUERYTHMO SE FERMERA.\nCette fenêtre affiche les logs de l'application qui seront nécessaires à envoyer au développeur au cas où vous rencontrez un bug !";
const CONSOLE_NOTICE_STYLE: &str = "\x1b[1;38;5;220m";
const CONSOLE_STYLE_RESET: &str = "\x1b[0m";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    eprintln!("{CONSOLE_NOTICE_STYLE}{CONSOLE_NOTICE}{CONSOLE_STYLE_RESET}");
    // Args are passed three ways:
    // 1. A `coquerythmo://` URL (registry protocol handler takes precedence).
    // 2. A `.coquerythmo` project path to import.
    // 3. Anything else (ignored, kept for future compatibility).
    let mut startup_url: Option<String> = None;
    let mut startup_path: Option<std::path::PathBuf> = None;
    for arg in std::env::args().skip(1) {
        if arg.starts_with(coquerythmo::protocol::PROTOCOL_PREFIX) {
            // Keep the URI as-is; don't PathBuf::from it (paths inside the
            // base64 payload would be mangled on Windows).
            startup_url = Some(arg);
            break;
        }
        let candidate = std::path::PathBuf::from(&arg);
        let matches_project = candidate
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case(coquerythmo::project_archive::PROJECT_EXTENSION)
            });
        if matches_project {
            startup_path = Some(candidate);
        }
    }
    let startup = startup_url
        .map(coquerythmo::app::StartupInput::Url)
        .or_else(|| startup_path.map(coquerythmo::app::StartupInput::Project));
    coquerythmo::app::run(startup);
}
