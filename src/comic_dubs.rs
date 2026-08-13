//! Durable Comic Dubs document: ordered pages, media and translated bubbles.

use crate::recording::{RecordedAudio, WaveformData};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

pub type PageId = u64;
pub type BubbleId = u64;
pub type ComicAudioId = u64;

pub(crate) fn bubble_playback_state(
    bubble: &Bubble,
    index: usize,
    visible_bubbles: usize,
) -> (bool, bool) {
    let revealed = index < visible_bubbles;
    let has_text = !bubble.text.trim().is_empty();
    (!revealed || has_text, revealed && has_text)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bubble {
    pub id: BubbleId,
    pub points: Vec<Point>,
    pub text: String,
    pub color: [u8; 4],
    #[serde(default = "default_bubble_font_size")]
    pub font_size: f32,
    #[serde(default)]
    pub audio_id: Option<ComicAudioId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub image_path: PathBuf,
    #[serde(default)]
    pub bubbles: Vec<Bubble>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComicAudio {
    pub id: ComicAudioId,
    pub file_name: String,
    #[serde(skip)]
    pub playback_path: PathBuf,
    pub sample_rate: u32,
    pub sample_count: u64,
    #[serde(skip)]
    pub waveform: WaveformData,
}

impl ComicAudio {
    pub fn duration_ms(&self) -> u64 {
        self.sample_count
            .saturating_mul(1_000)
            .checked_div(u64::from(self.sample_rate.max(1)))
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComicDubsProject {
    #[serde(default)]
    pages: Vec<Page>,
    #[serde(default)]
    audios: Vec<ComicAudio>,
    #[serde(default)]
    active_page: Option<PageId>,
    #[serde(default = "default_bubble_gap_ms")]
    bubble_gap_ms: u64,
    #[serde(default = "default_page_gap_ms")]
    page_gap_ms: u64,
    #[serde(default)]
    font_family: Option<String>,
    #[serde(default = "default_bubble_font_size")]
    default_font_size: f32,
    #[serde(default = "first_id")]
    next_id: u64,
}

const fn first_id() -> u64 {
    1
}

const fn default_bubble_gap_ms() -> u64 {
    250
}

const fn default_page_gap_ms() -> u64 {
    250
}

const fn default_bubble_font_size() -> f32 {
    24.0
}

impl Default for ComicDubsProject {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            audios: Vec::new(),
            active_page: None,
            bubble_gap_ms: default_bubble_gap_ms(),
            page_gap_ms: default_page_gap_ms(),
            font_family: None,
            default_font_size: default_bubble_font_size(),
            next_id: first_id(),
        }
    }
}

impl ComicDubsProject {
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn audios(&self) -> &[ComicAudio] {
        &self.audios
    }

    pub fn active_page_id(&self) -> Option<PageId> {
        self.active_page
    }

    pub fn active_page(&self) -> Option<&Page> {
        let id = self.active_page?;
        self.pages.iter().find(|page| page.id == id)
    }

    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    pub fn audio(&self, id: ComicAudioId) -> Option<&ComicAudio> {
        self.audios.iter().find(|audio| audio.id == id)
    }

    pub fn bubble(&self, id: BubbleId) -> Option<&Bubble> {
        self.pages
            .iter()
            .flat_map(|page| &page.bubbles)
            .find(|bubble| bubble.id == id)
    }

    pub fn bubble_gap_ms(&self) -> u64 {
        self.bubble_gap_ms
    }

    pub fn page_gap_ms(&self) -> u64 {
        self.page_gap_ms
    }

    pub fn font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    pub fn default_font_size(&self) -> f32 {
        self.default_font_size
    }

    pub fn set_settings(
        &mut self,
        font_family: Option<String>,
        bubble_gap_ms: u64,
        page_gap_ms: u64,
        default_font_size: f32,
    ) -> bool {
        if !default_font_size.is_finite() {
            return false;
        }
        let font_family = font_family.and_then(|font| {
            let font = clean_name(&font, "");
            (!font.is_empty()).then_some(font)
        });
        let settings = (
            font_family,
            bubble_gap_ms.min(60_000),
            page_gap_ms.min(60_000),
            default_font_size.clamp(6.0, 72.0),
        );
        if (
            &self.font_family,
            self.bubble_gap_ms,
            self.page_gap_ms,
            self.default_font_size,
        ) == (&settings.0, settings.1, settings.2, settings.3)
        {
            return false;
        }
        (
            self.font_family,
            self.bubble_gap_ms,
            self.page_gap_ms,
            self.default_font_size,
        ) = settings;
        true
    }

    pub fn set_gaps(&mut self, bubble_gap_ms: u64, page_gap_ms: u64) -> bool {
        let gaps = (bubble_gap_ms.min(60_000), page_gap_ms.min(60_000));
        if (self.bubble_gap_ms, self.page_gap_ms) == gaps {
            return false;
        }
        (self.bubble_gap_ms, self.page_gap_ms) = gaps;
        true
    }

    pub fn add_page(
        &mut self,
        file_name: String,
        image_path: PathBuf,
        width: u32,
        height: u32,
    ) -> PageId {
        let id = self.allocate_id();
        self.pages.push(Page {
            id,
            file_name: clean_name(&file_name, "page.png"),
            width: width.max(1),
            height: height.max(1),
            image_path,
            bubbles: Vec::new(),
        });
        self.active_page = Some(id);
        id
    }

    pub fn bind_page(&mut self, id: PageId, image_path: PathBuf) -> bool {
        let Some(page) = self.pages.iter_mut().find(|page| page.id == id) else {
            return false;
        };
        page.image_path = image_path;
        true
    }

    pub fn select_page(&mut self, id: PageId) -> bool {
        if self.active_page == Some(id) || self.page(id).is_none() {
            return false;
        }
        self.active_page = Some(id);
        true
    }

    pub fn remove_page(&mut self, id: PageId) -> bool {
        let Some(index) = self.pages.iter().position(|page| page.id == id) else {
            return false;
        };
        self.pages.remove(index);
        if self.active_page == Some(id) {
            self.active_page = self
                .pages
                .get(index.min(self.pages.len().saturating_sub(1)))
                .map(|page| page.id);
        }
        true
    }

    pub fn move_page(&mut self, id: PageId, delta: isize) -> bool {
        let Some(from) = self.pages.iter().position(|page| page.id == id) else {
            return false;
        };
        let to = from
            .saturating_add_signed(delta)
            .min(self.pages.len().saturating_sub(1));
        if from == to {
            return false;
        }
        let page = self.pages.remove(from);
        self.pages.insert(to, page);
        true
    }

    pub fn add_audio(
        &mut self,
        file_name: String,
        playback_path: PathBuf,
        recorded: RecordedAudio,
    ) -> ComicAudioId {
        let id = self.allocate_id();
        self.audios.push(ComicAudio {
            id,
            file_name: clean_name(&file_name, &recorded.file_name),
            playback_path,
            sample_rate: recorded.sample_rate,
            sample_count: recorded.sample_count,
            waveform: recorded.waveform,
        });
        id
    }

    pub fn bind_audio(
        &mut self,
        id: ComicAudioId,
        playback_path: PathBuf,
        recorded: RecordedAudio,
    ) -> bool {
        let Some(audio) = self.audios.iter_mut().find(|audio| audio.id == id) else {
            return false;
        };
        audio.playback_path = playback_path;
        audio.sample_rate = recorded.sample_rate;
        audio.sample_count = recorded.sample_count;
        audio.waveform = recorded.waveform;
        true
    }

    pub fn remove_audio(&mut self, id: ComicAudioId) -> bool {
        let before = self.audios.len();
        self.audios.retain(|audio| audio.id != id);
        if self.audios.len() == before {
            return false;
        }
        for page in &mut self.pages {
            for bubble in &mut page.bubbles {
                if bubble.audio_id == Some(id) {
                    bubble.audio_id = None;
                }
            }
        }
        true
    }

    pub fn add_bubble(&mut self, page_id: PageId, points: Vec<Point>) -> Option<BubbleId> {
        if !valid_polygon(&points) {
            return None;
        }
        let id = self.allocate_id();
        let font_size = self.default_font_size;
        let page = self.pages.iter_mut().find(|page| page.id == page_id)?;
        page.bubbles.push(Bubble {
            id,
            points,
            text: String::new(),
            color: [255, 255, 255, 255],
            font_size,
            audio_id: None,
        });
        Some(id)
    }

    pub fn set_bubble_text(&mut self, id: BubbleId, text: String) -> bool {
        let text = clean_name(&text, "");
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.text == text {
            return false;
        }
        bubble.text = text;
        true
    }

    pub fn set_bubble_color(&mut self, id: BubbleId, color: [u8; 4]) -> bool {
        let color = [color[0], color[1], color[2], 255];
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.color == color {
            return false;
        }
        bubble.color = color;
        true
    }

    pub fn set_bubble_font_size(&mut self, id: BubbleId, font_size: f32) -> bool {
        if !font_size.is_finite() {
            return false;
        }
        let font_size = font_size.clamp(6.0, 72.0);
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.font_size == font_size {
            return false;
        }
        bubble.font_size = font_size;
        true
    }

    pub fn set_bubble_points(&mut self, id: BubbleId, points: Vec<Point>) -> bool {
        if !valid_polygon(&points) {
            return false;
        }
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.points == points {
            return false;
        }
        bubble.points = points;
        true
    }

    pub fn assign_audio(&mut self, bubble_id: BubbleId, audio_id: Option<ComicAudioId>) -> bool {
        if audio_id.is_some_and(|id| self.audio(id).is_none()) {
            return false;
        }
        let Some(bubble) = self.bubble_mut(bubble_id) else {
            return false;
        };
        if bubble.audio_id == audio_id {
            return false;
        }
        bubble.audio_id = audio_id;
        true
    }

    pub fn remove_bubble(&mut self, id: BubbleId) -> bool {
        for page in &mut self.pages {
            let before = page.bubbles.len();
            page.bubbles.retain(|bubble| bubble.id != id);
            if page.bubbles.len() != before {
                return true;
            }
        }
        false
    }

    pub fn move_bubble(&mut self, id: BubbleId, delta: isize) -> bool {
        let Some(page) = self
            .pages
            .iter_mut()
            .find(|page| page.bubbles.iter().any(|bubble| bubble.id == id))
        else {
            return false;
        };
        let from = page
            .bubbles
            .iter()
            .position(|bubble| bubble.id == id)
            .unwrap();
        let to = from
            .saturating_add_signed(delta)
            .min(page.bubbles.len().saturating_sub(1));
        if from == to {
            return false;
        }
        let bubble = page.bubbles.remove(from);
        page.bubbles.insert(to, bubble);
        true
    }

    pub(crate) fn validate(&mut self) -> Result<(), String> {
        let mut ids = HashSet::new();
        let audio_ids = self
            .audios
            .iter()
            .map(|audio| audio.id)
            .collect::<HashSet<_>>();
        for page in &mut self.pages {
            if page.width == 0
                || page.height == 0
                || page.file_name.trim().is_empty()
                || !ids.insert(page.id)
            {
                return Err("invalid Comic Dubs page".into());
            }
            for bubble in &mut page.bubbles {
                bubble.color[3] = 255;
                if !ids.insert(bubble.id)
                    || !valid_polygon(&bubble.points)
                    || !bubble.font_size.is_finite()
                    || !(6.0..=72.0).contains(&bubble.font_size)
                    || bubble.audio_id.is_some_and(|id| !audio_ids.contains(&id))
                {
                    return Err("invalid Comic Dubs bubble".into());
                }
            }
        }
        for audio in &self.audios {
            if audio.sample_rate == 0
                || audio.sample_count == 0
                || audio.file_name.trim().is_empty()
                || !ids.insert(audio.id)
            {
                return Err("invalid Comic Dubs audio".into());
            }
        }
        if self
            .active_page
            .and_then(|id| self.pages.iter().find(|page| page.id == id))
            .is_none()
        {
            self.active_page = self.pages.first().map(|page| page.id);
        }
        self.bubble_gap_ms = self.bubble_gap_ms.min(60_000);
        self.page_gap_ms = self.page_gap_ms.min(60_000);
        self.font_family = self.font_family.take().and_then(|font| {
            let font = clean_name(&font, "");
            (!font.is_empty()).then_some(font)
        });
        if !self.default_font_size.is_finite() {
            self.default_font_size = default_bubble_font_size();
        }
        self.default_font_size = self.default_font_size.clamp(6.0, 72.0);
        self.next_id = self
            .next_id
            .max(ids.into_iter().max().unwrap_or(0).saturating_add(1));
        Ok(())
    }

    fn bubble_mut(&mut self, id: BubbleId) -> Option<&mut Bubble> {
        self.pages
            .iter_mut()
            .flat_map(|page| &mut page.bubbles)
            .find(|bubble| bubble.id == id)
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id.saturating_add(1);
        id
    }
}

fn clean_name(value: &str, fallback: &str) -> String {
    let value: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(500)
        .collect();
    let value = value.trim();
    if value.is_empty() {
        fallback.into()
    } else {
        value.into()
    }
}

fn valid_polygon(points: &[Point]) -> bool {
    points.len() >= 3
        && points.len() <= 128
        && points.iter().all(|point| {
            point.x.is_finite()
                && point.y.is_finite()
                && (0.0..=1.0).contains(&point.x)
                && (0.0..=1.0).contains(&point.y)
        })
        && polygon_area(points) > 0.000_01
}

fn polygon_area(points: &[Point]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>()
        .abs()
        * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Vec<Point> {
        vec![
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.9, y: 0.1 },
            Point { x: 0.5, y: 0.9 },
        ]
    }

    #[test]
    fn pages_and_bubbles_follow_explicit_reading_order() {
        let mut project = ComicDubsProject::default();
        let first = project.add_page("1.jpg".into(), "1.png".into(), 100, 200);
        let second = project.add_page("2.jpg".into(), "2.png".into(), 100, 200);
        assert!(project.move_page(second, -1));
        assert_eq!(project.pages()[0].id, second);

        let a = project.add_bubble(first, triangle()).unwrap();
        let b = project.add_bubble(first, triangle()).unwrap();
        assert!(project.move_bubble(b, -1));
        assert_eq!(project.page(first).unwrap().bubbles[0].id, b);
        assert_ne!(a, b);
    }

    #[test]
    fn removing_audio_unassigns_every_bubble() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        let audio = project.add_audio(
            "line.wav".into(),
            "line.flac".into(),
            RecordedAudio {
                file_name: "line.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        assert!(project.assign_audio(bubble, Some(audio)));
        assert!(project.remove_audio(audio));
        assert_eq!(project.bubble(bubble).unwrap().audio_id, None);
    }

    #[test]
    fn bubble_style_is_per_bubble_and_always_opaque() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        assert!(project.set_bubble_font_size(bubble, 36.0));
        assert!(project.set_bubble_color(bubble, [10, 20, 30, 1]));
        assert_eq!(project.bubble(bubble).unwrap().font_size, 36.0);
        assert_eq!(project.bubble(bubble).unwrap().color, [10, 20, 30, 255]);
    }

    #[test]
    fn settings_supply_new_bubble_defaults() {
        let mut project = ComicDubsProject::default();
        assert_eq!((project.bubble_gap_ms(), project.page_gap_ms()), (250, 250));
        assert!(project.set_settings(Some("Comic Sans MS".into()), 500, 750, 32.0));
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        assert_eq!(project.font_family(), Some("Comic Sans MS"));
        assert_eq!(project.bubble(bubble).unwrap().font_size, 32.0);

        let json = serde_json::to_string(&project).unwrap();
        let restored: ComicDubsProject = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.font_family(), Some("Comic Sans MS"));
        assert_eq!(
            (restored.bubble_gap_ms(), restored.page_gap_ms()),
            (500, 750)
        );
        assert_eq!(restored.default_font_size(), 32.0);
    }

    #[test]
    fn bubbles_may_have_no_text() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        assert!(project.bubble(bubble).unwrap().text.is_empty());
        assert!(project.set_bubble_text(bubble, "Texte".into()));
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 0),
            (true, false)
        );
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 1),
            (true, true)
        );
        assert!(project.set_bubble_text(bubble, String::new()));
        assert!(project.validate().is_ok());
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 0),
            (true, false)
        );
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 1),
            (false, false)
        );
    }
}
