//! Platform abstraction traits.
//!
//! Note storage, retrieval, and search operations.
//! Platform-specific implementations live in submodules.

use std::time::SystemTime;

#[cfg(target_os = "macos")]
pub mod macos;

/// Metadata for a note (lightweight, used in listings).
#[derive(Debug, Clone)]
pub struct NoteMeta {
    /// Unique note identifier (filename stem or UUID).
    pub id: String,
    /// Note title (first heading or filename).
    pub title: String,
    /// Last modification time.
    pub modified: SystemTime,
    /// Tags extracted from front-matter or content.
    pub tags: Vec<String>,
}

/// A full note with content.
#[derive(Debug, Clone)]
pub struct Note {
    /// Note metadata.
    pub meta: NoteMeta,
    /// Full note content (markdown).
    pub content: String,
}

/// Note storage and retrieval operations.
pub trait NoteStorage: Send + Sync {
    /// List all notes with metadata.
    fn list_notes(&self) -> Result<Vec<NoteMeta>, Box<dyn std::error::Error>>;

    /// Read a note by its ID.
    fn read_note(&self, id: &str) -> Result<Note, Box<dyn std::error::Error>>;

    /// Save (create or update) a note.
    fn save_note(&self, note: &Note) -> Result<(), Box<dyn std::error::Error>>;

    /// Delete a note by its ID.
    fn delete_note(&self, id: &str) -> Result<(), Box<dyn std::error::Error>>;

    /// Search notes by query string.
    fn search_notes(&self, query: &str) -> Result<Vec<NoteMeta>, Box<dyn std::error::Error>>;
}

/// Create a platform-specific note storage implementation.
pub fn create_storage(
    notes_dir: &std::path::Path,
) -> Box<dyn NoteStorage> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSNoteStorage::new(notes_dir.to_path_buf()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = notes_dir;
        panic!("note storage not implemented for this platform")
    }
}
