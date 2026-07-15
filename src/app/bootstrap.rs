//! Process/application initialization kept outside the event loop.

use crate::{config, i18n, platform, update};

/// Initialize configuration, translations and the updater using the same
/// order as the pre-refactor binary.
pub(crate) fn initialize() -> bool {
    config::init();
    i18n::init(&config::get().lang);
    update::promote_pending_updater_at_startup();
    platform::show_untested_platform_warning();
    update::check()
}
