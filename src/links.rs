//! Wiki link parsing and backlink tracking.
//!
//! Parses `[[link]]` and `[[link|display]]` syntax from markdown content,
//! resolves links against known note IDs/titles, and maintains a
//! bidirectional link index for backlink queries.

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// A parsed wiki link from note content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    /// The target note reference (what's inside `[[...]]`).
    pub target: String,
    /// Optional display text (after `|`).
    pub display: Option<String>,
    /// Optional heading anchor (after `#`).
    pub heading: Option<String>,
    /// Byte offset of the `[[` in the source text.
    pub start: usize,
    /// Byte offset of the `]]` end in the source text.
    pub end: usize,
}

impl WikiLink {
    /// The text to display for this link.
    #[must_use]
    pub fn display_text(&self) -> &str {
        self.display.as_deref().unwrap_or(&self.target)
    }
}

static WIKI_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Matches [[target]], [[target|display]], [[target#heading]], [[target#heading|display]]
    Regex::new(r"\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]")
        .expect("wiki link regex should compile")
});

/// Extract all wiki links from markdown content.
#[must_use]
pub fn extract_wiki_links(content: &str) -> Vec<WikiLink> {
    WIKI_LINK_RE
        .captures_iter(content)
        .map(|cap| {
            let full_match = cap.get(0).expect("full match should exist");
            WikiLink {
                target: cap[1].trim().to_string(),
                heading: cap.get(2).map(|m| m.as_str().trim().to_string()),
                display: cap.get(3).map(|m| m.as_str().trim().to_string()),
                start: full_match.start(),
                end: full_match.end(),
            }
        })
        .collect()
}

/// Check if the cursor position (byte offset) is inside a wiki link pattern `[[`.
/// Returns the partial target text typed so far if we're inside `[[...`.
#[must_use]
pub fn wiki_link_at_cursor(content: &str, cursor_byte: usize) -> Option<String> {
    let before = &content[..cursor_byte.min(content.len())];

    // Find the last `[[` before cursor
    let open_idx = before.rfind("[[")?;
    let after_open = &before[open_idx + 2..];

    // Check that there's no `]]` between `[[` and cursor
    if after_open.contains("]]") {
        return None;
    }

    // Return the partial text (what the user has typed so far)
    let partial = after_open.split('|').next().unwrap_or(after_open);
    Some(partial.to_string())
}

/// Bidirectional link index: tracks which notes link to which.
#[derive(Debug, Clone, Default)]
pub struct BacklinkIndex {
    /// Forward links: note_id -> set of note IDs it links to.
    forward: HashMap<String, HashSet<String>>,
    /// Reverse links: note_id -> set of note IDs that link to it.
    reverse: HashMap<String, HashSet<String>>,
}

impl BacklinkIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the index for a single note. Clears old links for this note
    /// and replaces with the new set.
    pub fn update_note(
        &mut self,
        source_id: &str,
        links: &[WikiLink],
        title_map: &HashMap<String, String>,
    ) {
        // Remove old forward links from reverse index
        if let Some(old_targets) = self.forward.remove(source_id) {
            for target in &old_targets {
                if let Some(backlinks) = self.reverse.get_mut(target) {
                    backlinks.remove(source_id);
                }
            }
        }

        // Build new forward links
        let mut targets = HashSet::new();
        for link in links {
            let resolved = resolve_link(&link.target, title_map);
            if let Some(target_id) = resolved {
                targets.insert(target_id.clone());
                self.reverse
                    .entry(target_id)
                    .or_default()
                    .insert(source_id.to_string());
            }
        }

        if !targets.is_empty() {
            self.forward.insert(source_id.to_string(), targets);
        }
    }

    /// Remove a note from the index entirely.
    pub fn remove_note(&mut self, note_id: &str) {
        // Remove forward links
        if let Some(targets) = self.forward.remove(note_id) {
            for target in &targets {
                if let Some(backlinks) = self.reverse.get_mut(target) {
                    backlinks.remove(note_id);
                }
            }
        }
        // Remove as a backlink target
        self.reverse.remove(note_id);
    }

    /// Get all notes that link TO the given note.
    #[must_use]
    pub fn backlinks_for(&self, note_id: &str) -> Vec<String> {
        self.reverse
            .get(note_id)
            .map(|set| {
                let mut v: Vec<String> = set.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    /// Get all notes that the given note links TO.
    #[must_use]
    pub fn forward_links_for(&self, note_id: &str) -> Vec<String> {
        self.forward
            .get(note_id)
            .map(|set| {
                let mut v: Vec<String> = set.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    /// Get orphan notes (notes with no incoming or outgoing links).
    #[must_use]
    pub fn orphans(&self, all_ids: &[String]) -> Vec<String> {
        all_ids
            .iter()
            .filter(|id| {
                let has_forward = self.forward.get(id.as_str()).is_some_and(|s| !s.is_empty());
                let has_reverse = self.reverse.get(id.as_str()).is_some_and(|s| !s.is_empty());
                !has_forward && !has_reverse
            })
            .cloned()
            .collect()
    }

    /// Total number of unique links in the index.
    #[must_use]
    pub fn link_count(&self) -> usize {
        self.forward.values().map(HashSet::len).sum()
    }
}

/// Resolve a wiki link target to a note ID using the title map.
/// Matches case-insensitively against filenames, titles, and aliases.
fn resolve_link(target: &str, title_map: &HashMap<String, String>) -> Option<String> {
    let lower = target.to_lowercase();
    title_map.get(&lower).cloned()
}

/// Find broken links in a note's content (links that don't resolve to any note).
#[must_use]
pub fn find_broken_links(
    links: &[WikiLink],
    title_map: &HashMap<String, String>,
) -> Vec<WikiLink> {
    links
        .iter()
        .filter(|link| resolve_link(&link.target, title_map).is_none())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_link() {
        let links = extract_wiki_links("See [[my-note]] for details.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "my-note");
        assert!(links[0].display.is_none());
        assert!(links[0].heading.is_none());
    }

    #[test]
    fn extract_link_with_display() {
        let links = extract_wiki_links("See [[my-note|My Note]] for details.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "my-note");
        assert_eq!(links[0].display.as_deref(), Some("My Note"));
    }

    #[test]
    fn extract_link_with_heading() {
        let links = extract_wiki_links("See [[my-note#section]] for details.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "my-note");
        assert_eq!(links[0].heading.as_deref(), Some("section"));
    }

    #[test]
    fn extract_link_with_heading_and_display() {
        let links = extract_wiki_links("See [[my-note#section|Display]] for details.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "my-note");
        assert_eq!(links[0].heading.as_deref(), Some("section"));
        assert_eq!(links[0].display.as_deref(), Some("Display"));
    }

    #[test]
    fn extract_multiple_links() {
        let links = extract_wiki_links("Link to [[a]] and [[b]] and [[c|See C]].");
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "a");
        assert_eq!(links[1].target, "b");
        assert_eq!(links[2].target, "c");
        assert_eq!(links[2].display.as_deref(), Some("See C"));
    }

    #[test]
    fn extract_no_links() {
        let links = extract_wiki_links("No wiki links here.");
        assert!(links.is_empty());
    }

    #[test]
    fn display_text_with_alias() {
        let link = WikiLink {
            target: "my-note".into(),
            display: Some("Custom Display".into()),
            heading: None,
            start: 0,
            end: 0,
        };
        assert_eq!(link.display_text(), "Custom Display");
    }

    #[test]
    fn display_text_without_alias() {
        let link = WikiLink {
            target: "my-note".into(),
            display: None,
            heading: None,
            start: 0,
            end: 0,
        };
        assert_eq!(link.display_text(), "my-note");
    }

    #[test]
    fn wiki_link_at_cursor_inside() {
        let content = "text [[my-no";
        let result = wiki_link_at_cursor(content, 12);
        assert_eq!(result.as_deref(), Some("my-no"));
    }

    #[test]
    fn wiki_link_at_cursor_outside() {
        let content = "text [[my-note]] more text";
        let result = wiki_link_at_cursor(content, 20);
        assert!(result.is_none());
    }

    #[test]
    fn wiki_link_at_cursor_no_open() {
        let result = wiki_link_at_cursor("just text", 5);
        assert!(result.is_none());
    }

    #[test]
    fn backlink_index_basic() {
        let mut idx = BacklinkIndex::new();
        let mut title_map = HashMap::new();
        title_map.insert("note-b".to_string(), "note-b".to_string());

        let links = vec![WikiLink {
            target: "note-b".into(),
            display: None,
            heading: None,
            start: 0,
            end: 0,
        }];

        idx.update_note("note-a", &links, &title_map);
        assert_eq!(idx.backlinks_for("note-b"), vec!["note-a"]);
        assert_eq!(idx.forward_links_for("note-a"), vec!["note-b"]);
    }

    #[test]
    fn backlink_index_update_replaces() {
        let mut idx = BacklinkIndex::new();
        let mut title_map = HashMap::new();
        title_map.insert("b".to_string(), "b".to_string());
        title_map.insert("c".to_string(), "c".to_string());

        // First: a -> b
        idx.update_note(
            "a",
            &[WikiLink {
                target: "b".into(),
                display: None,
                heading: None,
                start: 0,
                end: 0,
            }],
            &title_map,
        );
        assert_eq!(idx.backlinks_for("b"), vec!["a"]);

        // Update: a -> c (not b anymore)
        idx.update_note(
            "a",
            &[WikiLink {
                target: "c".into(),
                display: None,
                heading: None,
                start: 0,
                end: 0,
            }],
            &title_map,
        );
        assert!(idx.backlinks_for("b").is_empty());
        assert_eq!(idx.backlinks_for("c"), vec!["a"]);
    }

    #[test]
    fn backlink_index_remove() {
        let mut idx = BacklinkIndex::new();
        let mut title_map = HashMap::new();
        title_map.insert("b".to_string(), "b".to_string());

        idx.update_note(
            "a",
            &[WikiLink {
                target: "b".into(),
                display: None,
                heading: None,
                start: 0,
                end: 0,
            }],
            &title_map,
        );
        idx.remove_note("a");
        assert!(idx.backlinks_for("b").is_empty());
    }

    #[test]
    fn orphan_detection() {
        let mut idx = BacklinkIndex::new();
        let mut title_map = HashMap::new();
        title_map.insert("b".to_string(), "b".to_string());

        idx.update_note(
            "a",
            &[WikiLink {
                target: "b".into(),
                display: None,
                heading: None,
                start: 0,
                end: 0,
            }],
            &title_map,
        );

        let all_ids = vec!["a".into(), "b".into(), "c".into()];
        let orphans = idx.orphans(&all_ids);
        // "c" has no links at all
        assert_eq!(orphans, vec!["c"]);
    }

    #[test]
    fn find_broken_links_basic() {
        let title_map: HashMap<String, String> =
            [("existing".to_string(), "existing".to_string())]
                .into_iter()
                .collect();

        let links = vec![
            WikiLink {
                target: "existing".into(),
                display: None,
                heading: None,
                start: 0,
                end: 0,
            },
            WikiLink {
                target: "missing".into(),
                display: None,
                heading: None,
                start: 0,
                end: 0,
            },
        ];

        let broken = find_broken_links(&links, &title_map);
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].target, "missing");
    }

    #[test]
    fn link_count() {
        let mut idx = BacklinkIndex::new();
        let mut title_map = HashMap::new();
        title_map.insert("b".to_string(), "b".to_string());
        title_map.insert("c".to_string(), "c".to_string());

        idx.update_note(
            "a",
            &[
                WikiLink { target: "b".into(), display: None, heading: None, start: 0, end: 0 },
                WikiLink { target: "c".into(), display: None, heading: None, start: 0, end: 0 },
            ],
            &title_map,
        );
        assert_eq!(idx.link_count(), 2);
    }
}
