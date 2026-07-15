//! Ownership of modal instances.
//!
//! The existing modal controllers still live in their dedicated modules. The
//! host owns their lifetime and gives the shell one place to inspect whether a
//! modal captures input; migration of individual event/render branches can
//! therefore happen without changing modal state ownership again.

use super::connect_modal::ConnectModal;
use super::export_modal::ExportModal;
use super::file_explorer::{FileExplorerModal, FileExplorerRequest, FileExplorerResult};
use super::language_modal::{LanguageListItem, LanguageModal};
use super::pricing_license_modal::PricingLicenseModal;
use super::pricing_page::PricingPage;
use super::pricing_plan_modal::PricingPlanModal;
use super::primitives::{EventResponse, LabelInfo, QuadInstance, UiAction, UiEvent};
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

/// Result of dispatching an interaction to the active modal.
///
/// The host owns modal lifecycle; the shell only translates this small result
/// into the application's event response.
pub enum ModalOutcome {
    Consumed,
    Action(UiAction),
}

impl ModalOutcome {
    pub fn into_event_response(self) -> EventResponse {
        match self {
            Self::Consumed => EventResponse::Consumed,
            Self::Action(action) => EventResponse::Action(action),
        }
    }
}

pub struct ModalHost {
    pub connect: Option<ConnectModal>,
    pub settings: Option<SettingsModal>,
    pub project_settings: Option<ProjectSettingsModal>,
    pub export: Option<ExportModal>,
    pub languages: Option<LanguageModal>,
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
            languages: None,
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
            || self.languages.is_some()
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

    pub fn is_editing_text(&self) -> bool {
        self.file_explorer
            .as_ref()
            .is_some_and(|modal| modal.is_editing_text())
            || self.connect.is_some()
            || self.export.is_some()
            || self.languages.is_some()
            || self.proxy.is_some()
            || self.proxy_error.is_some()
            || self.voice_actor.is_some()
            || self.rename_character.is_some()
            || self.whats_new.is_some()
            || self.pricing_page.is_some()
            || self.pricing_plan.is_some()
            || self.pricing_license.is_some()
    }

    /// Route an event using the existing modal priority, keeping all modal
    /// lifecycle transitions in the modal host instead of the shell.
    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ModalOutcome> {
        if let Some(outcome) = self.handle_topmost_event(event, screen_w, screen_h) {
            return Some(outcome);
        }
        self.handle_secondary_event(event, screen_w, screen_h)
    }

    /// Route the modal layers that historically ran before toast dismissal.
    pub fn handle_topmost_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ModalOutcome> {
        if self.pricing_page.is_some() {
            return Some(self.handle_pricing_event(event, screen_w, screen_h));
        }
        if self.file_explorer.is_some() {
            return Some(self.handle_file_explorer_event(event, screen_w, screen_h));
        }
        if self.proxy_error.is_some() {
            return Some(self.handle_proxy_error_event(event, screen_w, screen_h));
        }
        if self.whats_new.is_some() {
            return Some(self.handle_whats_new_event(event, screen_w, screen_h));
        }
        None
    }

    fn handle_secondary_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ModalOutcome> {
        if self.settings.is_some() {
            return Some(self.handle_settings_event(event, screen_w, screen_h));
        }
        if self.project_settings.is_some() {
            return Some(self.handle_project_settings_event(event, screen_w, screen_h));
        }
        if self.studio_warning.is_some() {
            return Some(self.handle_studio_warning_event(event, screen_w, screen_h));
        }
        if self.save_prompt.is_some() {
            return Some(self.handle_save_prompt_event(event, screen_w, screen_h));
        }
        if self.export.is_some() {
            return Some(self.handle_export_event(event, screen_w, screen_h));
        }
        if self.languages.is_some() {
            return Some(self.handle_languages_event(event, screen_w, screen_h));
        }
        if self.voice_actor.is_some() {
            return Some(self.handle_voice_actor_event(event, screen_w, screen_h));
        }
        if self.rename_character.is_some() {
            return Some(self.handle_rename_character_event(event, screen_w, screen_h));
        }
        if self.proxy.is_some() {
            return Some(self.handle_proxy_event(event, screen_w, screen_h));
        }
        if self.add_server.is_some() {
            return Some(self.handle_add_server_event(event, screen_w, screen_h));
        }
        if self.server_browser.is_some() {
            return Some(self.handle_server_browser_event(event, screen_w, screen_h));
        }
        if self.connect.is_some() {
            return Some(self.handle_connect_event(event, screen_w, screen_h));
        }
        None
    }

    fn handle_connect_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .connect
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::connect_modal::ConnectModalResult::Consumed => ModalOutcome::Consumed,
            super::connect_modal::ConnectModalResult::Close => {
                self.connect = None;
                ModalOutcome::Consumed
            }
            super::connect_modal::ConnectModalResult::Connect {
                ip,
                port,
                password,
                username,
                room_code,
            } => {
                self.connect = None;
                ModalOutcome::Action(UiAction::NetworkConnect {
                    ip,
                    port,
                    password,
                    username,
                    room_code,
                })
            }
        }
    }

    fn handle_settings_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .settings
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::settings_modal::SettingsModalResult::Consumed => ModalOutcome::Consumed,
            super::settings_modal::SettingsModalResult::Close => {
                self.settings = None;
                ModalOutcome::Consumed
            }
            super::settings_modal::SettingsModalResult::Save {
                lang,
                rythmo_font,
                scroll_speed,
            } => {
                self.settings = None;
                ModalOutcome::Action(UiAction::SaveSettings {
                    lang,
                    rythmo_font,
                    scroll_speed,
                })
            }
        }
    }

    fn handle_project_settings_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .project_settings
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::project_settings_modal::ProjectSettingsModalResult::Consumed => {
                ModalOutcome::Consumed
            }
            super::project_settings_modal::ProjectSettingsModalResult::Close => {
                self.project_settings = None;
                ModalOutcome::Consumed
            }
            super::project_settings_modal::ProjectSettingsModalResult::PickInstrumentalAudio => {
                ModalOutcome::Action(UiAction::PickProjectInstrumentalAudio)
            }
            super::project_settings_modal::ProjectSettingsModalResult::Save {
                instrumental_audio_path,
                highlight_read_word,
            } => {
                self.project_settings = None;
                ModalOutcome::Action(UiAction::SaveProjectSettings {
                    instrumental_audio_path,
                    highlight_read_word,
                })
            }
        }
    }

    fn handle_export_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .export
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::export_modal::ExportModalResult::Consumed => ModalOutcome::Consumed,
            super::export_modal::ExportModalResult::Close { configuration } => {
                self.export = None;
                ModalOutcome::Action(UiAction::SaveExportConfiguration { configuration })
            }
            super::export_modal::ExportModalResult::Export { configuration } => {
                self.export = None;
                ModalOutcome::Action(UiAction::StartConfiguredExport { configuration })
            }
        }
    }

    fn handle_languages_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        use super::language_modal::LanguageModalResult;
        match self
            .languages
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            LanguageModalResult::Consumed => ModalOutcome::Consumed,
            LanguageModalResult::Close => {
                self.languages = None;
                ModalOutcome::Consumed
            }
            LanguageModalResult::Create { name } => {
                ModalOutcome::Action(UiAction::CreateLanguage { name })
            }
            LanguageModalResult::Rename { id, name } => {
                ModalOutcome::Action(UiAction::RenameLanguage { id, name })
            }
            LanguageModalResult::Delete { id } => {
                ModalOutcome::Action(UiAction::DeleteLanguage { id })
            }
            LanguageModalResult::Select { id } => {
                ModalOutcome::Action(UiAction::SelectLanguage { id })
            }
            LanguageModalResult::PickInstrumental { id } => {
                ModalOutcome::Action(UiAction::PickLanguageInstrumentalAudio { id })
            }
            LanguageModalResult::ClearInstrumental { id } => {
                ModalOutcome::Action(UiAction::ClearLanguageInstrumentalAudio { id })
            }
        }
    }

    fn handle_file_explorer_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .file_explorer
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            FileExplorerResult::Consumed => ModalOutcome::Consumed,
            FileExplorerResult::Close => {
                self.file_explorer = None;
                ModalOutcome::Consumed
            }
            FileExplorerResult::Clipboard(text) => {
                ModalOutcome::Action(UiAction::SetClipboard(text))
            }
            FileExplorerResult::Selected { intent, path } => {
                self.file_explorer = None;
                ModalOutcome::Action(UiAction::FilePickerSelected { intent, path })
            }
        }
    }

    fn handle_voice_actor_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .voice_actor
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::voice_actor_modal::VoiceActorModalResult::Consumed => ModalOutcome::Consumed,
            super::voice_actor_modal::VoiceActorModalResult::Close => {
                self.voice_actor = None;
                ModalOutcome::Consumed
            }
            super::voice_actor_modal::VoiceActorModalResult::PickIcon => {
                ModalOutcome::Action(UiAction::PickVoiceActorIcon)
            }
            super::voice_actor_modal::VoiceActorModalResult::Clipboard(text) => {
                ModalOutcome::Action(UiAction::SetClipboard(text))
            }
            super::voice_actor_modal::VoiceActorModalResult::Create { name, icon_path } => {
                self.voice_actor = None;
                ModalOutcome::Action(UiAction::CreateVoiceActor { name, icon_path })
            }
        }
    }

    fn handle_rename_character_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .rename_character
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::rename_character_modal::RenameCharacterModalResult::Consumed => {
                ModalOutcome::Consumed
            }
            super::rename_character_modal::RenameCharacterModalResult::Close => {
                self.rename_character = None;
                ModalOutcome::Consumed
            }
            super::rename_character_modal::RenameCharacterModalResult::Clipboard(text) => {
                ModalOutcome::Action(UiAction::SetClipboard(text))
            }
            super::rename_character_modal::RenameCharacterModalResult::Rename {
                old_name,
                new_name,
            } => {
                self.rename_character = None;
                ModalOutcome::Action(UiAction::RenameCharacter { old_name, new_name })
            }
        }
    }

    fn handle_proxy_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .proxy
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::proxy_modal::ProxyModalResult::Consumed => ModalOutcome::Consumed,
            super::proxy_modal::ProxyModalResult::Close => {
                self.proxy = None;
                ModalOutcome::Consumed
            }
            super::proxy_modal::ProxyModalResult::Create { width, height, crf } => {
                self.proxy = None;
                ModalOutcome::Action(UiAction::CreateProxy { width, height, crf })
            }
        }
    }

    fn handle_proxy_error_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .proxy_error
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::proxy_error_modal::ProxyErrorResult::Consumed => ModalOutcome::Consumed,
            super::proxy_error_modal::ProxyErrorResult::Close => {
                self.proxy_error = None;
                ModalOutcome::Consumed
            }
        }
    }

    fn handle_whats_new_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .whats_new
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::whats_new_modal::WhatsNewResult::Consumed => ModalOutcome::Consumed,
            super::whats_new_modal::WhatsNewResult::Close => {
                self.whats_new = None;
                ModalOutcome::Consumed
            }
        }
    }

    fn handle_server_browser_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .server_browser
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::server_browser::BrowserResult::Consumed => ModalOutcome::Consumed,
            super::server_browser::BrowserResult::Close => {
                self.server_browser = None;
                ModalOutcome::Consumed
            }
            super::server_browser::BrowserResult::CreateRoom { ip, port } => {
                self.server_browser = None;
                ModalOutcome::Action(UiAction::OpenConnectModal {
                    ip,
                    port,
                    join: false,
                })
            }
            super::server_browser::BrowserResult::JoinRoom { ip, port } => {
                self.server_browser = None;
                ModalOutcome::Action(UiAction::OpenConnectModal {
                    ip,
                    port,
                    join: true,
                })
            }
            super::server_browser::BrowserResult::AddServer => {
                ModalOutcome::Action(UiAction::OpenAddServerModal)
            }
            super::server_browser::BrowserResult::RemoveServer(index) => {
                ModalOutcome::Action(UiAction::RemoveServer(index))
            }
            super::server_browser::BrowserResult::Refresh => {
                ModalOutcome::Action(UiAction::RefreshServers)
            }
        }
    }

    fn handle_add_server_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .add_server
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::server_browser::AddServerResult::Consumed => ModalOutcome::Consumed,
            super::server_browser::AddServerResult::Close => {
                self.add_server = None;
                ModalOutcome::Consumed
            }
            super::server_browser::AddServerResult::Add { ip, port } => {
                self.add_server = None;
                ModalOutcome::Action(UiAction::AddServer { ip, port })
            }
        }
    }

    fn handle_save_prompt_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        let kind = self.save_prompt.as_ref().unwrap().kind();
        match self
            .save_prompt
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::save_prompt_modal::SavePromptResult::Consumed => ModalOutcome::Consumed,
            super::save_prompt_modal::SavePromptResult::Save => {
                self.save_prompt = None;
                ModalOutcome::Action(match kind {
                    super::save_prompt_modal::SavePromptKind::NewProject => {
                        UiAction::NewProjectSave
                    }
                    super::save_prompt_modal::SavePromptKind::CloseProject => {
                        UiAction::CloseProjectSave
                    }
                    super::save_prompt_modal::SavePromptKind::ExitApplication => {
                        UiAction::ExitApplicationSave
                    }
                })
            }
            super::save_prompt_modal::SavePromptResult::Discard => {
                self.save_prompt = None;
                ModalOutcome::Action(match kind {
                    super::save_prompt_modal::SavePromptKind::NewProject => {
                        UiAction::NewProjectDiscard
                    }
                    super::save_prompt_modal::SavePromptKind::CloseProject => {
                        UiAction::CloseProjectDiscard
                    }
                    super::save_prompt_modal::SavePromptKind::ExitApplication => {
                        UiAction::ExitApplicationDiscard
                    }
                })
            }
            super::save_prompt_modal::SavePromptResult::Cancel => {
                self.save_prompt = None;
                ModalOutcome::Consumed
            }
        }
    }

    fn handle_studio_warning_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        match self
            .studio_warning
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::studio_warning_modal::StudioWarningResult::Consumed => ModalOutcome::Consumed,
            super::studio_warning_modal::StudioWarningResult::Confirm => {
                self.studio_warning = None;
                ModalOutcome::Action(UiAction::EnterStudioMode)
            }
            super::studio_warning_modal::StudioWarningResult::Cancel => {
                self.studio_warning = None;
                ModalOutcome::Consumed
            }
        }
    }

    fn handle_pricing_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        if let Some(modal) = &mut self.pricing_license {
            return match modal.handle_event(event, screen_w, screen_h) {
                super::pricing_license_modal::PricingLicenseModalResult::Consumed => {
                    ModalOutcome::Consumed
                }
                super::pricing_license_modal::PricingLicenseModalResult::Close => {
                    self.pricing_license = None;
                    ModalOutcome::Consumed
                }
                super::pricing_license_modal::PricingLicenseModalResult::Activate(key) => {
                    self.pricing_license = None;
                    ModalOutcome::Action(UiAction::ActivateLicense { key })
                }
            };
        }

        if let Some(modal) = &mut self.pricing_plan {
            return match modal.handle_event(event, screen_w, screen_h) {
                super::pricing_plan_modal::PricingPlanModalResult::Consumed => {
                    ModalOutcome::Consumed
                }
                super::pricing_plan_modal::PricingPlanModalResult::Close => {
                    self.pricing_plan = None;
                    ModalOutcome::Consumed
                }
                super::pricing_plan_modal::PricingPlanModalResult::Confirm(plan) => {
                    self.pricing_plan = None;
                    ModalOutcome::Action(UiAction::SubscribePlan { plan })
                }
            };
        }

        match self
            .pricing_page
            .as_mut()
            .unwrap()
            .handle_event(event, screen_w, screen_h)
        {
            super::pricing_page::PricingResult::Consumed => ModalOutcome::Consumed,
            super::pricing_page::PricingResult::Close => {
                self.pricing_page = None;
                ModalOutcome::Consumed
            }
            super::pricing_page::PricingResult::SelectPlan(plan) => {
                self.pricing_plan = Some(super::pricing_plan_modal::PricingPlanModal::new(
                    plan.name().to_string(),
                    plan.price().to_string(),
                    plan.is_enterprise(),
                ));
                ModalOutcome::Consumed
            }
            super::pricing_page::PricingResult::ActivateLicense => {
                self.pricing_license =
                    Some(super::pricing_license_modal::PricingLicenseModal::new());
                ModalOutcome::Consumed
            }
        }
    }

    pub fn open_export(
        &mut self,
        video_width: u32,
        video_height: u32,
        languages: Vec<super::export_modal::ExportLanguageOption>,
        configuration: crate::project::ExportConfiguration,
    ) {
        self.export = Some(super::export_modal::ExportModal::new(
            video_width,
            video_height,
            languages,
            configuration,
        ));
    }

    pub fn open_languages(&mut self, languages: Vec<LanguageListItem>, active_language_id: u64) {
        self.languages = Some(LanguageModal::new(languages, active_language_id));
    }

    pub fn refresh_languages(&mut self, languages: Vec<LanguageListItem>, active_language_id: u64) {
        if let Some(modal) = &mut self.languages {
            modal.refresh(languages, active_language_id);
        }
    }

    pub fn open_file_explorer(&mut self, request: FileExplorerRequest) {
        self.file_explorer = Some(FileExplorerModal::new(request));
    }

    pub fn poll_file_explorer(&mut self) -> bool {
        self.file_explorer
            .as_mut()
            .is_some_and(|modal| modal.poll_background())
    }

    pub fn open_voice_actor(&mut self) {
        self.voice_actor = Some(super::voice_actor_modal::VoiceActorModal::new());
    }

    pub fn open_rename_character(&mut self, characters: Vec<String>) {
        self.rename_character = Some(super::rename_character_modal::RenameCharacterModal::new(
            characters,
        ));
    }

    pub fn set_voice_actor_icon_path(&mut self, path: impl Into<String>) {
        if let Some(modal) = &mut self.voice_actor {
            modal.set_icon_path(path);
        }
    }

    pub fn open_proxy(&mut self, video_width: u32, video_height: u32) {
        self.proxy = Some(super::proxy_modal::ProxyModal::new(
            video_width,
            video_height,
        ));
    }

    pub fn open_proxy_error(&mut self, detail: impl Into<String>) {
        self.proxy_error = Some(super::proxy_error_modal::ProxyErrorModal::new(detail));
    }

    pub fn open_whats_new(&mut self, version: impl Into<String>, body: impl Into<String>) {
        self.whats_new = Some(super::whats_new_modal::WhatsNewModal::new(version, body));
    }

    pub fn open_save_prompt(&mut self, kind: super::save_prompt_modal::SavePromptKind) {
        self.save_prompt = Some(super::save_prompt_modal::SavePromptModal::new(kind));
    }

    pub fn open_studio_warning(&mut self) {
        self.studio_warning = Some(super::studio_warning_modal::StudioWarningModal::new());
    }

    pub fn open_pricing_page(&mut self) {
        self.pricing_page = Some(super::pricing_page::PricingPage::new());
    }

    pub fn close_pricing_page(&mut self) {
        self.pricing_page = None;
        self.pricing_plan = None;
        self.pricing_license = None;
    }

    pub fn open_server_browser(&mut self) {
        self.server_browser = Some(super::server_browser::ServerBrowserModal::new());
    }

    pub fn open_add_server(&mut self) {
        self.add_server = Some(super::server_browser::AddServerModal::new());
    }

    pub fn server_browser_mut(&mut self) -> Option<&mut ServerBrowserModal> {
        self.server_browser.as_mut()
    }

    pub fn open_connect(&mut self, ip: &str, port: u16, join: bool) {
        self.connect = Some(super::connect_modal::ConnectModal::new_with_server(
            ip, port, join,
        ));
    }

    pub fn open_settings(&mut self, fonts: Vec<String>) {
        self.settings = Some(super::settings_modal::SettingsModal::new(fonts));
    }

    pub fn open_project_settings(
        &mut self,
        instrumental_audio_path: Option<String>,
        highlight_read_word: bool,
    ) {
        self.project_settings = Some(super::project_settings_modal::ProjectSettingsModal::new(
            instrumental_audio_path,
            highlight_read_word,
        ));
    }

    pub fn set_project_instrumental_audio_path(&mut self, path: impl Into<String>) {
        if let Some(modal) = &mut self.project_settings {
            modal.set_instrumental_audio_path(path);
        }
    }

    pub fn close_project_settings(&mut self) {
        self.project_settings = None;
    }

    pub fn close_settings(&mut self) {
        self.settings = None;
    }

    pub fn render_pricing<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        modal_quads: &mut Vec<QuadInstance>,
        modal_labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        if let Some(page) = &self.pricing_page {
            page.render(quads, overlay_quads, labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.pricing_plan {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.pricing_license {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
    }

    /// Render the modal layers that sit below loading/export overlays.
    pub fn render_base<'a>(
        &'a self,
        modal_quads: &mut Vec<QuadInstance>,
        modal_labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        if let Some(modal) = &self.settings {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.project_settings {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.connect {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.server_browser {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.add_server {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.export {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.languages {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.voice_actor {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.rename_character {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.proxy {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.save_prompt {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.studio_warning {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
    }

    /// Render the modal layers that must remain above transient overlays.
    pub fn render_top<'a>(
        &'a self,
        modal_quads: &mut Vec<QuadInstance>,
        modal_labels: &mut Vec<LabelInfo<'a>>,
        modal_overlay_quads: &mut Vec<QuadInstance>,
        modal_overlay_labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        if let Some(modal) = &self.whats_new {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.proxy_error {
            modal.render(modal_quads, modal_labels, screen_w, screen_h);
        }
        if let Some(modal) = &self.file_explorer {
            modal.render(
                modal_quads,
                modal_labels,
                modal_overlay_quads,
                modal_overlay_labels,
                screen_w,
                screen_h,
            );
        }
    }
}

impl Default for ModalHost {
    fn default() -> Self {
        Self::new()
    }
}
