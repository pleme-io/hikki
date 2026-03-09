//! Full-text search via tankyu (tantivy wrapper).
//!
//! Indexes note content, title, tags, and path for fast vault-wide search.
//! Index is stored in `~/.cache/hikki/index/` and rebuilt from source files
//! when stale or missing.

use std::path::{Path, PathBuf};
use thiserror::Error;
use tankyu::{IndexStore, SchemaSpec, STORED, STRING, TEXT};

use crate::notes::{Note, NoteError, Vault};

/// Errors from search operations.
#[derive(Error, Debug)]
pub enum SearchError {
    #[error("index error: {0}")]
    Index(#[from] tankyu::TankyuError),
    #[error("note error: {0}")]
    Note(#[from] NoteError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// A search result with score and note metadata.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Note ID (relative path without .md).
    pub id: String,
    /// Note title.
    pub title: String,
    /// Search relevance score.
    pub score: f32,
}

/// Full-text search index for a note vault.
pub struct SearchIndex {
    store: IndexStore,
}

impl SearchIndex {
    /// Open or create a search index at the given directory.
    pub fn open(index_dir: impl AsRef<Path>) -> Result<Self, SearchError> {
        let spec = Self::schema();
        let store = IndexStore::open(index_dir, &spec)?;
        Ok(Self { store })
    }

    /// Get the default index directory for hikki.
    #[must_use]
    pub fn default_index_dir() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("hikki")
            .join("index")
    }

    fn schema() -> SchemaSpec {
        SchemaSpec::new()
            .field("id", STRING | STORED)
            .field("title", TEXT | STORED)
            .field("body", TEXT)
            .field("tags", TEXT | STORED)
    }

    /// Index a single note.
    pub fn index_note(&self, note: &Note) -> Result<(), SearchError> {
        self.store.write(|w| {
            // Delete old entry for this note
            w.delete_term("id", &note.meta.id);
            // Add updated entry
            let tags = note.meta.tags.join(" ");
            let body = crate::notes::strip_front_matter(&note.content);
            w.add_doc(&[
                ("id", &note.meta.id),
                ("title", &note.meta.title),
                ("body", body),
                ("tags", &tags),
            ])?;
            Ok(())
        })?;
        Ok(())
    }

    /// Re-index all notes in a vault from scratch.
    pub fn reindex_vault(&self, vault: &Vault) -> Result<usize, SearchError> {
        let notes = vault.list_notes()?;
        let mut count = 0;

        self.store.write(|w| {
            w.delete_all()?;
            Ok(())
        })?;

        for meta in &notes {
            if let Ok(note) = vault.read_note(&meta.id) {
                self.store.write_no_commit(|w| {
                    let tags = note.meta.tags.join(" ");
                    let body = crate::notes::strip_front_matter(&note.content);
                    w.add_doc(&[
                        ("id", &note.meta.id),
                        ("title", &note.meta.title),
                        ("body", body),
                        ("tags", &tags),
                    ])?;
                    Ok(())
                })?;
                count += 1;
            }
        }

        self.store.commit()?;
        tracing::info!(count, "vault reindexed");
        Ok(count)
    }

    /// Search the index with a query string.
    pub fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>, SearchError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let results = self.store.search(query, &["title", "body", "tags"], max_results)?;
        Ok(results
            .into_iter()
            .filter_map(|(score, doc)| {
                let id = doc.get("id")?.as_text()?.to_string();
                let title = doc
                    .get("title")
                    .and_then(|v| v.as_text())
                    .unwrap_or(&id)
                    .to_string();
                Some(SearchResult { id, title, score })
            })
            .collect())
    }

    /// Search for notes with a specific tag.
    pub fn search_tag(&self, tag: &str, max_results: usize) -> Result<Vec<SearchResult>, SearchError> {
        self.search(tag, max_results)
    }

    /// Remove a note from the index.
    pub fn remove_note(&self, id: &str) -> Result<(), SearchError> {
        self.store.write(|w| {
            w.delete_term("id", id);
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Vault, SearchIndex) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("notes")).unwrap();
        let index = SearchIndex::open(dir.path().join("index")).unwrap();
        (dir, vault, index)
    }

    #[test]
    fn index_and_search() {
        let (_dir, vault, index) = setup();
        vault.create_note("rust-guide", "Rust Programming Guide").unwrap();
        vault.create_note("python-intro", "Python Introduction").unwrap();

        let note1 = vault.read_note("rust-guide").unwrap();
        let note2 = vault.read_note("python-intro").unwrap();
        index.index_note(&note1).unwrap();
        index.index_note(&note2).unwrap();

        let results = index.search("rust", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rust-guide");
    }

    #[test]
    fn reindex_vault() {
        let (_dir, vault, index) = setup();
        vault.create_note("note-a", "Alpha Note").unwrap();
        vault.create_note("note-b", "Beta Note").unwrap();

        let count = index.reindex_vault(&vault).unwrap();
        assert_eq!(count, 2);

        let results = index.search("alpha", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_empty_query() {
        let (_dir, _vault, index) = setup();
        let results = index.search("", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn remove_note_from_index() {
        let (_dir, vault, index) = setup();
        vault.create_note("to-remove", "Remove Me").unwrap();
        let note = vault.read_note("to-remove").unwrap();
        index.index_note(&note).unwrap();

        let results = index.search("remove", 10).unwrap();
        assert_eq!(results.len(), 1);

        index.remove_note("to-remove").unwrap();
        let results = index.search("remove", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn default_index_dir() {
        let dir = SearchIndex::default_index_dir();
        assert!(dir.to_string_lossy().contains("hikki"));
    }
}
