//! macOS note storage implementation (filesystem-backed markdown).

use std::fs;
use std::path::PathBuf;

use crate::platform::{Note, NoteMeta, NoteStorage};

/// macOS-specific note storage using the local filesystem.
pub struct MacOSNoteStorage {
    notes_dir: PathBuf,
}

impl MacOSNoteStorage {
    pub fn new(notes_dir: PathBuf) -> Self {
        // Ensure the notes directory exists
        fs::create_dir_all(&notes_dir).ok();
        Self { notes_dir }
    }

    fn note_path(&self, id: &str) -> PathBuf {
        self.notes_dir.join(format!("{id}.md"))
    }

    fn extract_title(content: &str) -> String {
        // Use first heading or first line as title
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(heading) = trimmed.strip_prefix("# ") {
                return heading.to_string();
            }
            if !trimmed.is_empty() {
                return trimmed.chars().take(80).collect();
            }
        }
        String::from("Untitled")
    }

    fn extract_tags(content: &str) -> Vec<String> {
        // Simple tag extraction: lines starting with "tags:" in front-matter
        let mut tags = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(tag_line) = trimmed.strip_prefix("tags:") {
                for tag in tag_line.split(',') {
                    let tag = tag.trim().to_string();
                    if !tag.is_empty() {
                        tags.push(tag);
                    }
                }
                break;
            }
        }
        tags
    }
}

impl NoteStorage for MacOSNoteStorage {
    fn list_notes(&self) -> Result<Vec<NoteMeta>, Box<dyn std::error::Error>> {
        let mut notes = Vec::new();
        if !self.notes_dir.exists() {
            return Ok(notes);
        }
        for entry in fs::read_dir(&self.notes_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                let content = fs::read_to_string(&path)?;
                let metadata = entry.metadata()?;
                let id = path
                    .file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                notes.push(NoteMeta {
                    id,
                    title: Self::extract_title(&content),
                    modified: metadata.modified()?,
                    tags: Self::extract_tags(&content),
                });
            }
        }
        notes.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(notes)
    }

    fn read_note(&self, id: &str) -> Result<Note, Box<dyn std::error::Error>> {
        let path = self.note_path(id);
        let content = fs::read_to_string(&path)?;
        let metadata = fs::metadata(&path)?;
        Ok(Note {
            meta: NoteMeta {
                id: id.to_string(),
                title: Self::extract_title(&content),
                modified: metadata.modified()?,
                tags: Self::extract_tags(&content),
            },
            content,
        })
    }

    fn save_note(&self, note: &Note) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.note_path(&note.meta.id);
        fs::write(&path, &note.content)?;
        Ok(())
    }

    fn delete_note(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.note_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn search_notes(&self, query: &str) -> Result<Vec<NoteMeta>, Box<dyn std::error::Error>> {
        let query_lower = query.to_lowercase();
        let all_notes = self.list_notes()?;
        Ok(all_notes
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&query_lower)
                    || n.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect())
    }
}
