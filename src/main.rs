fn main() {
    env_logger::init();
    let startup_path = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .find(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case(coquerythmo::project_archive::PROJECT_EXTENSION)
                })
        });
    coquerythmo::app::run(startup_path);
}
