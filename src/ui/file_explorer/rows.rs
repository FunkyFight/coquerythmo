//! Row model: identifiers and flattening of the tree for rendering.

use std::ops::Range;

use crate::project::MediaId;

use super::data::FileTreeData;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupKind {
    Videos,
    Bands,
    Audios,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AudioRowId {
    /// Virtual fixed row: "Audio of the original video".
    OriginalVideo,
    Media(MediaId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowId {
    Root,
    Group(GroupKind),
    Video(MediaId),
    Audio(AudioRowId),
    Band(u64),
}

/// One flattened row of the visible tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: RowId,
    /// 0 for the root, 1 for groups, 2 for entries, 3 for proxy children.
    pub depth: u32,
}

impl Row {
    pub fn is_container(&self) -> bool {
        matches!(self.id, RowId::Root | RowId::Group(_))
    }
}

/// Flatten `data` into the visible rows given the expanded-group set.
///
/// Layout: root, then the three groups (Vidéos, Bandes rythmo, Audios); an
/// expanded group lists its entries. Proxy videos render as children of
/// their source video (depth + 1) regardless of their library vector order.
pub fn flatten(data: &FileTreeData, expanded: &ExpandedSet) -> Vec<Row> {
    let mut rows = vec![Row {
        id: RowId::Root,
        depth: 0,
    }];
    push_group(&mut rows, GroupKind::Videos, expanded.videos, || {
        let mut out = Vec::new();
        for video in data.videos.iter().filter(|v| v.proxy_of.is_none()) {
            out.push(Row {
                id: RowId::Video(video.id),
                depth: 2,
            });
            for proxy in data.videos.iter().filter(|v| v.proxy_of == Some(video.id)) {
                out.push(Row {
                    id: RowId::Video(proxy.id),
                    depth: 3,
                });
            }
        }
        out
    });
    push_group(&mut rows, GroupKind::Bands, expanded.bands, || {
        data.bands
            .iter()
            .map(|band| Row {
                id: RowId::Band(band.id),
                depth: 2,
            })
            .collect()
    });
    push_group(&mut rows, GroupKind::Audios, expanded.audios, || {
        data.audios
            .iter()
            .map(|audio| Row {
                id: RowId::Audio(audio.id),
                depth: 2,
            })
            .collect()
    });
    rows
}

fn push_group(
    rows: &mut Vec<Row>,
    kind: GroupKind,
    is_expanded: bool,
    entries: impl FnOnce() -> Vec<Row>,
) {
    rows.push(Row {
        id: RowId::Group(kind),
        depth: 1,
    });
    if is_expanded {
        rows.extend(entries());
    }
}

/// Expansion state of the three groups (root is never collapsible).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExpandedSet {
    pub videos: bool,
    pub bands: bool,
    pub audios: bool,
}

impl ExpandedSet {
    pub fn all_expanded() -> Self {
        Self {
            videos: true,
            bands: true,
            audios: true,
        }
    }

    pub fn get(&self, kind: GroupKind) -> bool {
        match kind {
            GroupKind::Videos => self.videos,
            GroupKind::Bands => self.bands,
            GroupKind::Audios => self.audios,
        }
    }

    pub fn toggle(&mut self, kind: GroupKind) {
        match kind {
            GroupKind::Videos => self.videos = !self.videos,
            GroupKind::Bands => self.bands = !self.bands,
            GroupKind::Audios => self.audios = !self.audios,
        }
    }
}

/// Position-in-set / size-of-set for a11y tree items among visible siblings.
pub fn set_metrics(rows: &[Row], id: RowId) -> Option<(usize, usize)> {
    let index = rows.iter().position(|row| row.id == id)?;
    let siblings: Vec<usize> = match rows[index].id {
        RowId::Root => vec![index],
        RowId::Group(_) => rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| matches!(row.id, RowId::Group(_)).then_some(index))
            .collect(),
        RowId::Video(_) if rows[index].depth == 3 => proxy_sibling_range(rows, index)
            .map(|range| range.collect())
            .unwrap_or_else(|| vec![index]),
        RowId::Video(_) => rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (matches!(row.id, RowId::Video(_)) && row.depth == 2).then_some(index)
            })
            .collect(),
        RowId::Audio(_) => rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| matches!(row.id, RowId::Audio(_)).then_some(index))
            .collect(),
        RowId::Band(_) => rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| matches!(row.id, RowId::Band(_)).then_some(index))
            .collect(),
    };
    let position = siblings.iter().position(|sibling| *sibling == index)?;
    Some((position + 1, siblings.len()))
}

/// The content range after `kind`'s header, stopping before the next header.
pub fn group_content_range(rows: &[Row], kind: GroupKind) -> Option<Range<usize>> {
    let start = rows.iter().position(|row| row.id == RowId::Group(kind))? + 1;
    let end = rows
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, row)| matches!(row.id, RowId::Group(_)).then_some(index))
        .unwrap_or(rows.len());
    Some(start..end)
}

fn proxy_sibling_range(rows: &[Row], index: usize) -> Option<Range<usize>> {
    let parent = (0..index).rev().find(|candidate| {
        rows[*candidate].depth == 2 && matches!(rows[*candidate].id, RowId::Video(_))
    })?;
    let end = rows
        .iter()
        .enumerate()
        .skip(index)
        .find_map(|(candidate, row)| (row.depth < 3).then_some(candidate))
        .unwrap_or(rows.len());
    Some((parent + 1)..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use crate::ui::file_explorer::data::FileTreeData;

    fn sample() -> FileTreeData {
        let mut project = Project::new();
        let source = project
            .add_media_video("Film", "C:/v/film.mp4", None, false)
            .unwrap();
        project
            .add_media_video("Proxy", "C:/v/p.mp4", Some(source), true)
            .unwrap();
        project.add_media_audio("Inst", "C:/a/i.wav").unwrap();
        project.create_language_named("English");
        FileTreeData::from_project(&project, "P", None, None)
    }

    #[test]
    fn collapsed_groups_show_only_root_and_headers() {
        let data = sample();
        let rows = flatten(&data, &ExpandedSet::default());
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].id, RowId::Root);
        assert_eq!(rows[1].id, RowId::Group(GroupKind::Videos));
        assert_eq!(rows[2].id, RowId::Group(GroupKind::Bands));
        assert_eq!(rows[3].id, RowId::Group(GroupKind::Audios));
    }

    #[test]
    fn expanded_videos_nest_proxies_under_source() {
        let data = sample();
        let expanded = ExpandedSet {
            videos: true,
            ..ExpandedSet::default()
        };
        let rows = flatten(&data, &expanded);
        let ids: Vec<RowId> = rows.iter().map(|r| r.id).collect();
        assert!(ids.contains(&RowId::Video(1)));
        assert!(ids.contains(&RowId::Video(2)));
        let source = rows.iter().find(|r| r.id == RowId::Video(1)).unwrap();
        let proxy = rows.iter().find(|r| r.id == RowId::Video(2)).unwrap();
        assert_eq!(source.depth, 2);
        assert_eq!(proxy.depth, 3);
        // proxy row must come directly after its source
        let source_index = ids.iter().position(|i| *i == RowId::Video(1)).unwrap();
        assert_eq!(ids[source_index + 1], RowId::Video(2));
    }

    #[test]
    fn fully_expanded_lists_every_entry() {
        let data = sample();
        let rows = flatten(&data, &ExpandedSet::all_expanded());
        // root + 3 groups + 2 videos + 2 bands + 1 audio = 9
        assert_eq!(rows.len(), 9);
    }

    #[test]
    fn set_metrics_are_one_based_within_visible_siblings() {
        let data = sample();
        let rows = flatten(&data, &ExpandedSet::all_expanded());
        let (position, size) = set_metrics(&rows, RowId::Root).unwrap();
        assert_eq!(position, 1);
        assert_eq!(size, 1);
        let (position, size) = set_metrics(&rows, RowId::Group(GroupKind::Bands)).unwrap();
        assert_eq!((position, size), (2, 3));
        assert!(set_metrics(&rows, RowId::Video(999)).is_none());
    }

    #[test]
    fn group_content_ranges_stop_at_the_next_group_header() {
        let data = sample();
        let rows = flatten(&data, &ExpandedSet::all_expanded());
        let videos = group_content_range(&rows, GroupKind::Videos).unwrap();
        assert_eq!(videos, 2..4);
        let audios = group_content_range(&rows, GroupKind::Audios).unwrap();
        assert_eq!(audios, 8..9);
    }
}
