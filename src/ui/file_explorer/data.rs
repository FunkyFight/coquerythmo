//! Immutable snapshot of everything the file tree needs to render.

use crate::i18n::t;
use crate::project::{MediaId, Project, SyllableLanguage};

use super::rows::AudioRowId;

/// Display model for one audio entry in the Audios group.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioData {
    pub id: AudioRowId,
    pub name: String,
    /// None for the virtual "original video audio" line (never stored).
    pub media_id: Option<MediaId>,
    /// File path (empty for the virtual original-video row).
    pub path: String,
    /// Names of the rythmo bands using this audio as their instrumental.
    pub instrumental_of: Vec<String>,
    /// Pre-rendered right-hand badge ("Instrumental : X, Y").
    pub instrumental_badge: String,
}

/// Display model for one video entry (top-level or proxy child).
#[derive(Clone, Debug, PartialEq)]
pub struct VideoData {
    pub id: MediaId,
    pub name: String,
    pub path: String,
    /// Some(source) => this video is the proxy of `source`.
    pub proxy_of: Option<MediaId>,
    pub generated: bool,
    /// This video is the one currently loaded for playback.
    pub active: bool,
    /// This video is the project default.
    pub default: bool,
    /// The file is missing on disk (warning style).
    pub missing: bool,
    /// True when this video is the default (alias for `default`).
    pub is_default: bool,
    /// True when another video is a proxy of this one.
    pub is_proxy_source: bool,
}

/// Display model for one rythmo band entry.
#[derive(Clone, Debug, PartialEq)]
pub struct BandData {
    pub id: u64,
    pub name: String,
    /// Currently selected band.
    pub active: bool,
    pub syllable_language: SyllableLanguage,
    pub instrumental_audio_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileTreeData {
    /// Project root label (`.coquerythmo` stem, fallback i18n "Sans titre").
    pub root_name: String,
    pub videos: Vec<VideoData>,
    pub audios: Vec<AudioData>,
    pub bands: Vec<BandData>,
    /// Whether the video preview currently shows a proxy.
    pub active_proxy: bool,
}

impl FileTreeData {
    /// Build the immutable snapshot from a project plus session state.
    ///
    /// `source_path`/`proxy_path` describe the currently loaded video (the
    /// active row is matched by path), `default_video` is the library default.
    pub fn from_project(
        project: &Project,
        root_name: impl Into<String>,
        source_path: Option<&str>,
        proxy_path: Option<&str>,
    ) -> Self {
        let library = project.media_library();
        let default_video = library.default_video;

        let videos = library
            .videos
            .iter()
            .map(|video| {
                let path = video.path.as_str();
                let active = source_path
                    .map(|p| paths_equal(p, path))
                    .unwrap_or(false)
                    || proxy_path
                        .map(|p| paths_equal(p, path))
                        .unwrap_or(false);
                VideoData {
                    id: video.id,
                    name: video.name.clone(),
                    path: video.path.clone(),
                    proxy_of: video.proxy_of,
                    generated: video.generated,
                    active,
                    default: default_video == Some(video.id),
                    missing: !std::path::Path::new(path).exists(),
                    is_default: default_video == Some(video.id),
                    is_proxy_source: library
                        .videos
                        .iter()
                        .any(|v| v.proxy_of == Some(video.id)),
                }
            })
            .collect();

        // Distinct instrumental paths across bands, in library order first
        // then any band-only leftovers (should not happen post-migration).
        let mut audios: Vec<AudioData> = Vec::new();
        for audio in &library.audios {
            let instrumental_of: Vec<String> = project
                .languages()
                .into_iter()
                .filter_map(|language| {
                    let path = project.language_instrumental_audio_path(language.id)?;
                    paths_equal(&path, &audio.path).then_some(language.name)
                })
                .collect();
            let instrumental_badge = if instrumental_of.is_empty() {
                String::new()
            } else {
                format!(
                    "{}: {}",
                    t("file_tree.badges.instrumental_of"),
                    instrumental_of.join(", ")
                )
            };
            audios.push(AudioData {
                id: AudioRowId::Media(audio.id),
                name: audio.name.clone(),
                media_id: Some(audio.id),
                path: audio.path.clone(),
                instrumental_of,
                instrumental_badge,
            });
        }

        let show_original = source_path.is_some();
        if show_original {
            audios.insert(
                0,
                AudioData {
                    id: AudioRowId::OriginalVideo,
                    name: String::new(),
                    media_id: None,
                    path: String::new(),
                    instrumental_of: Vec::new(),
                    instrumental_badge: String::new(),
                },
            );
        }

        let bands = project
            .languages()
            .into_iter()
            .map(|language| BandData {
                id: language.id,
                name: language.name,
                active: language.id == project.active_language_id(),
                syllable_language: project
                    .language_syllable_language(language.id)
                    .unwrap_or_default(),
                instrumental_audio_path: project
                    .language_instrumental_audio_path(language.id),
            })
            .collect();

        Self {
            root_name: root_name.into(),
            videos,
            audios,
            bands,
            active_proxy: proxy_path.is_some(),
        }
    }

    /// Names of bands whose instrumental matches `path`.
    pub fn instrumental_owners(&self, path: &str) -> Vec<&str> {
        self.bands
            .iter()
            .filter_map(|band| {
                band.instrumental_audio_path
                    .as_deref()
                    .map(|p| paths_equal(p, path))
                    .unwrap_or(false)
                    .then_some(band.name.as_str())
            })
            .collect()
    }
}

pub fn paths_equal(a: &str, b: &str) -> bool {
    std::path::Path::new(a) == std::path::Path::new(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_marks_active_default_and_proxy_rows() {
        let mut project = crate::project::Project::new();
        let source = project
            .add_media_video("Film", "C:/v/film.mp4", None, false)
            .unwrap();
        let proxy = project
            .add_media_video("Proxy", "C:/v/film_proxy.mp4", Some(source), true)
            .unwrap();
        project.set_default_video(Some(proxy)).unwrap();

        let data = FileTreeData::from_project(
            &project,
            "Projet",
            Some("C:/v/film.mp4"),
            Some("C:/v/film_proxy.mp4"),
        );

        let source_row = data.videos.iter().find(|v| v.id == source).unwrap();
        assert!(source_row.active);
        assert!(!source_row.default);
        let proxy_row = data.videos.iter().find(|v| v.id == proxy).unwrap();
        assert!(proxy_row.active);
        assert!(proxy_row.default);
        assert_eq!(proxy_row.proxy_of, Some(source));
        assert!(data.active_proxy);
    }

    #[test]
    fn original_audio_row_is_virtual_and_first() {
        let project = crate::project::Project::new();
        let data = FileTreeData::from_project(&project, "P", Some("C:/v/a.mp4"), None);
        assert_eq!(data.audios.first().map(|a| a.id), Some(AudioRowId::OriginalVideo));
        assert!(data.audios.first().unwrap().media_id.is_none());

        let empty = FileTreeData::from_project(&project, "P", None, None);
        assert!(empty.audios.is_empty());
    }

    #[test]
    fn instrumental_badges_match_band_paths() {
        let mut project = crate::project::Project::new();
        let fr = project.active_language_id();
        let en = project.create_language_named("English");
        let audio = project.add_media_audio("Inst", "C:/a/inst.wav").unwrap();
        project.set_language_instrumental_audio_path(fr, Some("C:/a/inst.wav".into()));
        project.set_language_instrumental_audio_path(en, Some("C:/a/inst.wav".into()));

        let data = FileTreeData::from_project(&project, "P", None, None);
        let row = data
            .audios
            .iter()
            .find(|a| a.id == AudioRowId::Media(audio))
            .unwrap();
        assert_eq!(row.instrumental_of.len(), 2);
        let owners = data.instrumental_owners("C:/a/inst.wav");
        assert_eq!(owners.len(), 2);
    }
}
