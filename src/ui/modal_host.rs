//! Ownership of modal instances.
//!
//! The existing modal controllers still live in their dedicated modules. The
//! host owns their lifetime and gives the shell one place to inspect whether a
//! modal captures input; migration of individual event/render branches can
//! therefore happen without changing modal state ownership again.

use super::connect_modal::ConnectModal;
use super::export_modal::ExportModal;
use super::file_explorer_modal::FileExplorerModal;
use super::pricing_license_modal::PricingLicenseModal;
use super::pricing_page::PricingPage;
use super::pricing_plan_modal::PricingPlanModal;
use super::project_settings_modal::ProjectSettingsModal;
use super::proxy_error_modal::ProxyErrorModal;
use super::proxy_modal::ProxyModal;
use super::rename_character_modal::RenameCharacterModal;
use super::save_prompt_modal::SavePromptModal;
use super::server_browser::{AddServerModal, ServerBrowserModal};
use super::settings_modal::SettingsModal;
use super::studio_warning_modal::StudioWarningModal;
use super::voice_actor_modal::VoiceActorModal;
use super::whats_new_modal::WhatsNewModal;

pub struct ModalHost {
    pub connect: Option<ConnectModal>,
    pub settings: Option<SettingsModal>,
    pub project_settings: Option<ProjectSettingsModal>,
    pub export: Option<ExportModal>,
    pub file_explorer: Option<FileExplorerModal>,
    pub proxy: Option<ProxyModal>,
    pub rename_character: Option<RenameCharacterModal>,
    pub proxy_error: Option<ProxyErrorModal>,
    pub server_browser: Option<ServerBrowserModal>,
    pub add_server: Option<AddServerModal>,
    pub save_prompt: Option<SavePromptModal>,
    pub studio_warning: Option<StudioWarningModal>,
    pub voice_actor: Option<VoiceActorModal>,
    pub whats_new: Option<WhatsNewModal>,
    pub pricing_page: Option<PricingPage>,
    pub pricing_plan: Option<PricingPlanModal>,
    pub pricing_license: Option<PricingLicenseModal>,
}

impl ModalHost {
    pub fn new() -> Self {
        Self {
            connect: None,
            settings: None,
            project_settings: None,
            export: None,
            file_explorer: None,
            proxy: None,
            rename_character: None,
            proxy_error: None,
            server_browser: None,
            add_server: None,
            save_prompt: None,
            studio_warning: None,
            voice_actor: None,
            whats_new: None,
            pricing_page: None,
            pricing_plan: None,
            pricing_license: None,
        }
    }

    pub fn captures_input(&self) -> bool {
        self.connect.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.export.is_some()
            || self.file_explorer.is_some()
            || self.proxy.is_some()
            || self.rename_character.is_some()
            || self.proxy_error.is_some()
            || self.server_browser.is_some()
            || self.add_server.is_some()
            || self.save_prompt.is_some()
            || self.studio_warning.is_some()
            || self.voice_actor.is_some()
            || self.whats_new.is_some()
            || self.pricing_page.is_some()
            || self.pricing_plan.is_some()
            || self.pricing_license.is_some()
    }
}

impl Default for ModalHost {
    fn default() -> Self {
        Self::new()
    }
}
