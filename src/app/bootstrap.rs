//! Process/application initialization kept outside the event loop.

use crate::{config, i18n, platform, update};

/// Initialize configuration, translations and the updater using the same
/// order as the pre-refactor binary.
pub(crate) fn initialize() -> bool {
    config::init();
    i18n::init(&config::get().lang);
    if let Err(error) = crate::project_archive::cleanup_project_extraction_at_startup() {
        log::warn!("Could not clean the project extraction directory at startup: {error}");
    }
    update::promote_pending_updater_at_startup();
    platform::show_untested_platform_warning();
    let updater_started = update::check();
    if !updater_started {
        platform::register_project_file_association();
        platform::register_url_protocol();
    }
    updater_started
}
