//! Text editing buffer with cursor, selection, and undo/redo.
//!
//! Uses a rope (ropey) for efficient text manipulation at any position.
//! Operation-based undo groups rapid keystrokes by coalescing within 500ms.

use ropey::Rope;
use std::time::Instant;

/// A single editing operation for undo/redo.
#[derive(Debug, Clone)]
enum EditOp {
    /// Insert text at a byte offset.
    Insert { char_offset: usize, text: String },
    /// Delete text at a byte offset.
    Delete { char_offset: usize, text: String },
}

impl EditOp {
    fn apply(&self, rope: &mut Rope) {
        match self {
            Self::Insert { char_offset, text } => {
                rope.insert(*char_offset, text);
            }
            Self::Delete { char_offset, text } => {
                let end = *char_offset + text.chars().count();
                rope.remove(*char_offset..end);
            }
        }
    }

    fn reverse(&self) -> Self {
        match self {
            Self::Insert { char_offset, text } => Self::Delete {
                char_offset: *char_offset,
                text: text.clone(),
            },
            Self::Delete { char_offset, text } => Self::Insert {
                char_offset: *char_offset,
                text: text.clone(),
            },
        }
    }
}

/// A group of edit operations that form a single undo step.
#[derive(Debug, Clone)]
struct UndoGroup {
    ops: Vec<EditOp>,
    timestamp: Instant,
}

/// Cursor position within the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Line number (0-based).
    pub line: usize,
    /// Column (char offset within line, 0-based).
    pub col: usize,
}

impl Cursor {
    #[must_use]
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self { line: 0, col: 0 }
    }
}

/// Selection range (start and end cursors, inclusive of start, exclusive of end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start: Cursor,
    pub end: Cursor,
}

impl Selection {
    /// Returns the selection with start <= end (normalized).
    #[must_use]
    pub fn normalized(&self) -> Self {
        if self.start.line > self.end.line
            || (self.start.line == self.end.line && self.start.col > self.end.col)
        {
            Self {
                start: self.end,
                end: self.start,
            }
        } else {
            *self
        }
    }
}

/// Main text editing buffer.
pub struct EditorBuffer {
    rope: Rope,
    cursor: Cursor,
    selection: Option<Selection>,
    undo_stack: Vec<UndoGroup>,
    redo_stack: Vec<UndoGroup>,
    /// Whether the buffer has been modified since last save.
    modified: bool,
    /// File path associated with this buffer.
    file_path: Option<String>,
    /// Coalescing window for undo groups (500ms).
    coalesce_ms: u128,
}

impl EditorBuffer {
    /// Create an empty editor buffer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            cursor: Cursor::default(),
            selection: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            modified: false,
            file_path: None,
            coalesce_ms: 500,
        }
    }

    /// Create an editor buffer from existing text content.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            cursor: Cursor::default(),
            selection: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            modified: false,
            file_path: None,
            coalesce_ms: 500,
        }
    }

    /// Get the full text content as a String.
    #[must_use]
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Get the cursor position.
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Get the current selection, if any.
    #[must_use]
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Whether the buffer has been modified since last save.
    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Mark the buffer as saved (unmodified).
    pub fn mark_saved(&mut self) {
        self.modified = false;
    }

    /// Get the file path associated with this buffer.
    #[must_use]
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// Set the file path.
    pub fn set_file_path(&mut self, path: impl Into<String>) {
        self.file_path = Some(path.into());
    }

    /// Total number of lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Get the text of a specific line (0-indexed), without trailing newline.
    #[must_use]
    pub fn line_text(&self, line_idx: usize) -> String {
        if line_idx >= self.rope.len_lines() {
            return String::new();
        }
        let line = self.rope.line(line_idx);
        let s = line.to_string();
        s.trim_end_matches('\n').trim_end_matches('\r').to_string()
    }

    /// Get the char count of a specific line (excluding trailing newline).
    #[must_use]
    pub fn line_len(&self, line_idx: usize) -> usize {
        self.line_text(line_idx).chars().count()
    }

    /// Convert cursor to a char offset in the rope.
    fn cursor_to_char_offset(&self, cursor: &Cursor) -> usize {
        if cursor.line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }
        let line_start = self.rope.line_to_char(cursor.line);
        let line_len = self.line_len(cursor.line);
        line_start + cursor.col.min(line_len)
    }

    /// Convert a char offset back to a Cursor.
    fn char_offset_to_cursor(&self, offset: usize) -> Cursor {
        let offset = offset.min(self.rope.len_chars());
        let line = self.rope.char_to_line(offset);
        let line_start = self.rope.line_to_char(line);
        let col = offset - line_start;
        Cursor::new(line, col)
    }

    /// Clamp cursor to valid position.
    fn clamp_cursor(&mut self) {
        let max_line = self.rope.len_lines().saturating_sub(1);
        self.cursor.line = self.cursor.line.min(max_line);
        let line_len = self.line_len(self.cursor.line);
        self.cursor.col = self.cursor.col.min(line_len);
    }

    // --- Text Operations ---

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        self.delete_selection_text();
        let offset = self.cursor_to_char_offset(&self.cursor);
        let text = ch.to_string();
        let op = EditOp::Insert {
            char_offset: offset,
            text: text.clone(),
        };
        op.apply(&mut self.rope);
        self.push_undo(op);

        // Move cursor forward
        if ch == '\n' {
            self.cursor.line += 1;
            self.cursor.col = 0;
        } else {
            self.cursor.col += 1;
        }
        self.modified = true;
        self.redo_stack.clear();
    }

    /// Insert a string at the cursor position.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.delete_selection_text();
        let offset = self.cursor_to_char_offset(&self.cursor);
        let op = EditOp::Insert {
            char_offset: offset,
            text: text.to_string(),
        };
        op.apply(&mut self.rope);
        self.push_undo(op);

        // Move cursor to end of inserted text
        let new_offset = offset + text.chars().count();
        self.cursor = self.char_offset_to_cursor(new_offset);
        self.modified = true;
        self.redo_stack.clear();
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_back(&mut self) {
        if self.delete_selection_text() {
            return;
        }
        let offset = self.cursor_to_char_offset(&self.cursor);
        if offset == 0 {
            return;
        }
        let prev_offset = offset - 1;
        let ch = self.rope.char(prev_offset);
        let text = ch.to_string();
        let op = EditOp::Delete {
            char_offset: prev_offset,
            text,
        };
        op.apply(&mut self.rope);
        self.push_undo(op);
        self.cursor = self.char_offset_to_cursor(prev_offset);
        self.modified = true;
        self.redo_stack.clear();
    }

    /// Delete the character at the cursor (delete key).
    pub fn delete_forward(&mut self) {
        if self.delete_selection_text() {
            return;
        }
        let offset = self.cursor_to_char_offset(&self.cursor);
        if offset >= self.rope.len_chars() {
            return;
        }
        let ch = self.rope.char(offset);
        let text = ch.to_string();
        let op = EditOp::Delete {
            char_offset: offset,
            text,
        };
        op.apply(&mut self.rope);
        self.push_undo(op);
        self.clamp_cursor();
        self.modified = true;
        self.redo_stack.clear();
    }

    /// Delete the entire current line.
    pub fn delete_line(&mut self) {
        if self.rope.len_lines() == 0 {
            return;
        }
        let line_start = self.rope.line_to_char(self.cursor.line);
        let line_end = if self.cursor.line + 1 < self.rope.len_lines() {
            self.rope.line_to_char(self.cursor.line + 1)
        } else {
            self.rope.len_chars()
        };
        if line_start == line_end {
            return;
        }
        let text: String = self.rope.slice(line_start..line_end).to_string();
        let op = EditOp::Delete {
            char_offset: line_start,
            text,
        };
        op.apply(&mut self.rope);
        self.push_undo(op);
        self.cursor.col = 0;
        self.clamp_cursor();
        self.modified = true;
        self.redo_stack.clear();
    }

    /// Yank (copy) the current line and return it.
    #[must_use]
    pub fn yank_line(&self) -> String {
        self.line_text(self.cursor.line) + "\n"
    }

    /// Paste text below the current line.
    pub fn paste_below(&mut self, text: &str) {
        let next_line_start = if self.cursor.line + 1 < self.rope.len_lines() {
            self.rope.line_to_char(self.cursor.line + 1)
        } else {
            self.rope.len_chars()
        };

        let insert_text = if self.cursor.line + 1 >= self.rope.len_lines() {
            format!("\n{text}")
        } else {
            text.to_string()
        };

        let op = EditOp::Insert {
            char_offset: next_line_start,
            text: insert_text,
        };
        op.apply(&mut self.rope);
        self.push_undo(op);
        self.cursor.line += 1;
        self.cursor.col = 0;
        self.modified = true;
        self.redo_stack.clear();
    }

    // --- Cursor Movement ---

    /// Move cursor left.
    pub fn move_left(&mut self) {
        self.selection = None;
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_len(self.cursor.line);
        }
    }

    /// Move cursor right.
    pub fn move_right(&mut self) {
        self.selection = None;
        let line_len = self.line_len(self.cursor.line);
        if self.cursor.col < line_len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.rope.len_lines() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    /// Move cursor up.
    pub fn move_up(&mut self) {
        self.selection = None;
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let line_len = self.line_len(self.cursor.line);
            self.cursor.col = self.cursor.col.min(line_len);
        }
    }

    /// Move cursor down.
    pub fn move_down(&mut self) {
        self.selection = None;
        if self.cursor.line + 1 < self.rope.len_lines() {
            self.cursor.line += 1;
            let line_len = self.line_len(self.cursor.line);
            self.cursor.col = self.cursor.col.min(line_len);
        }
    }

    /// Move cursor to start of line.
    pub fn move_to_line_start(&mut self) {
        self.selection = None;
        self.cursor.col = 0;
    }

    /// Move cursor to end of line.
    pub fn move_to_line_end(&mut self) {
        self.selection = None;
        self.cursor.col = self.line_len(self.cursor.line);
    }

    /// Move cursor to start of document.
    pub fn move_to_doc_start(&mut self) {
        self.selection = None;
        self.cursor.line = 0;
        self.cursor.col = 0;
    }

    /// Move cursor to end of document.
    pub fn move_to_doc_end(&mut self) {
        self.selection = None;
        self.cursor.line = self.rope.len_lines().saturating_sub(1);
        self.cursor.col = self.line_len(self.cursor.line);
    }

    /// Move cursor forward by one word.
    pub fn move_word_forward(&mut self) {
        self.selection = None;
        let line = self.line_text(self.cursor.line);
        let chars: Vec<char> = line.chars().collect();

        if self.cursor.col >= chars.len() {
            // Move to next line
            if self.cursor.line + 1 < self.rope.len_lines() {
                self.cursor.line += 1;
                self.cursor.col = 0;
            }
            return;
        }

        let mut col = self.cursor.col;
        // Skip current word chars
        while col < chars.len() && !chars[col].is_whitespace() {
            col += 1;
        }
        // Skip whitespace
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }
        self.cursor.col = col;
    }

    /// Move cursor backward by one word.
    pub fn move_word_backward(&mut self) {
        self.selection = None;
        if self.cursor.col == 0 {
            if self.cursor.line > 0 {
                self.cursor.line -= 1;
                self.cursor.col = self.line_len(self.cursor.line);
            }
            return;
        }

        let line = self.line_text(self.cursor.line);
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor.col;

        // Skip whitespace backward
        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        // Skip word chars backward
        while col > 0 && !chars[col - 1].is_whitespace() {
            col -= 1;
        }
        self.cursor.col = col;
    }

    /// Move cursor half-page down.
    pub fn move_half_page_down(&mut self, visible_lines: usize) {
        self.selection = None;
        let half = visible_lines / 2;
        let max_line = self.rope.len_lines().saturating_sub(1);
        self.cursor.line = (self.cursor.line + half).min(max_line);
        let line_len = self.line_len(self.cursor.line);
        self.cursor.col = self.cursor.col.min(line_len);
    }

    /// Move cursor half-page up.
    pub fn move_half_page_up(&mut self, visible_lines: usize) {
        self.selection = None;
        let half = visible_lines / 2;
        self.cursor.line = self.cursor.line.saturating_sub(half);
        let line_len = self.line_len(self.cursor.line);
        self.cursor.col = self.cursor.col.min(line_len);
    }

    /// Set cursor to a specific position.
    pub fn set_cursor(&mut self, line: usize, col: usize) {
        self.cursor.line = line.min(self.rope.len_lines().saturating_sub(1));
        self.cursor.col = col.min(self.line_len(self.cursor.line));
    }

    // --- Selection ---

    /// Start or extend selection from current cursor.
    pub fn start_selection(&mut self) {
        if self.selection.is_none() {
            self.selection = Some(Selection {
                start: self.cursor,
                end: self.cursor,
            });
        }
    }

    /// Extend selection to current cursor position.
    pub fn extend_selection_to_cursor(&mut self) {
        if let Some(ref mut sel) = self.selection {
            sel.end = self.cursor;
        }
    }

    /// Clear selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Get the selected text, if any.
    #[must_use]
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.selection?.normalized();
        let start = self.cursor_to_char_offset(&sel.start);
        let end = self.cursor_to_char_offset(&sel.end);
        if start == end {
            return None;
        }
        Some(self.rope.slice(start..end).to_string())
    }

    /// Delete the current selection and return whether it was deleted.
    fn delete_selection_text(&mut self) -> bool {
        let Some(sel) = self.selection.take() else {
            return false;
        };
        let sel = sel.normalized();
        let start = self.cursor_to_char_offset(&sel.start);
        let end = self.cursor_to_char_offset(&sel.end);
        if start == end {
            return false;
        }
        let text: String = self.rope.slice(start..end).to_string();
        let op = EditOp::Delete {
            char_offset: start,
            text,
        };
        op.apply(&mut self.rope);
        self.push_undo(op);
        self.cursor = sel.start;
        self.modified = true;
        self.redo_stack.clear();
        true
    }

    // --- Undo/Redo ---

    fn push_undo(&mut self, op: EditOp) {
        let now = Instant::now();
        if let Some(last) = self.undo_stack.last_mut() {
            let elapsed = now.duration_since(last.timestamp).as_millis();
            if elapsed < self.coalesce_ms {
                last.ops.push(op);
                last.timestamp = now;
                return;
            }
        }
        self.undo_stack.push(UndoGroup {
            ops: vec![op],
            timestamp: now,
        });
    }

    /// Undo the last edit group.
    pub fn undo(&mut self) {
        let Some(group) = self.undo_stack.pop() else {
            return;
        };
        let mut reverse_ops = Vec::new();
        for op in group.ops.iter().rev() {
            let rev = op.reverse();
            rev.apply(&mut self.rope);
            reverse_ops.push(op.clone());
        }
        self.redo_stack.push(UndoGroup {
            ops: reverse_ops,
            timestamp: group.timestamp,
        });
        self.clamp_cursor();
        self.modified = true;
    }

    /// Redo the last undone edit group.
    pub fn redo(&mut self) {
        let Some(group) = self.redo_stack.pop() else {
            return;
        };
        let mut redo_ops = Vec::new();
        for op in &group.ops {
            op.apply(&mut self.rope);
            redo_ops.push(op.clone());
        }
        self.undo_stack.push(UndoGroup {
            ops: redo_ops,
            timestamp: group.timestamp,
        });
        self.clamp_cursor();
        self.modified = true;
    }

    // --- Line Operations ---

    /// Insert a new line below cursor and move cursor to it.
    pub fn open_line_below(&mut self) {
        let line_end_offset = if self.cursor.line + 1 < self.rope.len_lines() {
            self.rope.line_to_char(self.cursor.line + 1)
        } else {
            self.rope.len_chars()
        };

        let op = EditOp::Insert {
            char_offset: line_end_offset,
            text: "\n".to_string(),
        };
        op.apply(&mut self.rope);
        self.push_undo(op);

        self.cursor.line += 1;
        self.cursor.col = 0;
        self.modified = true;
        self.redo_stack.clear();
    }

    /// Insert a new line above cursor and move cursor to it.
    pub fn open_line_above(&mut self) {
        let line_start = self.rope.line_to_char(self.cursor.line);
        let op = EditOp::Insert {
            char_offset: line_start,
            text: "\n".to_string(),
        };
        op.apply(&mut self.rope);
        self.push_undo(op);
        // cursor stays on the same line number, which is now the new blank line
        self.cursor.col = 0;
        self.modified = true;
        self.redo_stack.clear();
    }
}

impl Default for EditorBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer() {
        let buf = EditorBuffer::new();
        assert_eq!(buf.text(), "");
        assert_eq!(buf.cursor(), Cursor::new(0, 0));
        assert_eq!(buf.line_count(), 1); // ropey always has at least 1 line
        assert!(!buf.is_modified());
    }

    #[test]
    fn from_text() {
        let buf = EditorBuffer::from_text("hello\nworld");
        assert_eq!(buf.text(), "hello\nworld");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_text(0), "hello");
        assert_eq!(buf.line_text(1), "world");
    }

    #[test]
    fn insert_char_basic() {
        let mut buf = EditorBuffer::new();
        buf.insert_char('h');
        buf.insert_char('i');
        assert_eq!(buf.text(), "hi");
        assert_eq!(buf.cursor(), Cursor::new(0, 2));
        assert!(buf.is_modified());
    }

    #[test]
    fn insert_newline() {
        let mut buf = EditorBuffer::from_text("hello");
        buf.set_cursor(0, 5);
        buf.insert_char('\n');
        assert_eq!(buf.text(), "hello\n");
        assert_eq!(buf.cursor(), Cursor::new(1, 0));
    }

    #[test]
    fn delete_back() {
        let mut buf = EditorBuffer::from_text("abc");
        buf.set_cursor(0, 3);
        buf.delete_back();
        assert_eq!(buf.text(), "ab");
        assert_eq!(buf.cursor(), Cursor::new(0, 2));
    }

    #[test]
    fn delete_back_at_start() {
        let mut buf = EditorBuffer::from_text("abc");
        buf.set_cursor(0, 0);
        buf.delete_back();
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn delete_forward() {
        let mut buf = EditorBuffer::from_text("abc");
        buf.set_cursor(0, 0);
        buf.delete_forward();
        assert_eq!(buf.text(), "bc");
    }

    #[test]
    fn delete_line() {
        let mut buf = EditorBuffer::from_text("line1\nline2\nline3");
        buf.set_cursor(1, 0);
        buf.delete_line();
        assert_eq!(buf.text(), "line1\nline3");
    }

    #[test]
    fn cursor_movement() {
        let mut buf = EditorBuffer::from_text("hello\nworld");
        buf.set_cursor(0, 0);
        buf.move_right();
        assert_eq!(buf.cursor(), Cursor::new(0, 1));
        buf.move_down();
        assert_eq!(buf.cursor(), Cursor::new(1, 1));
        buf.move_left();
        assert_eq!(buf.cursor(), Cursor::new(1, 0));
        buf.move_up();
        assert_eq!(buf.cursor(), Cursor::new(0, 0));
    }

    #[test]
    fn move_to_line_start_end() {
        let mut buf = EditorBuffer::from_text("hello world");
        buf.set_cursor(0, 5);
        buf.move_to_line_start();
        assert_eq!(buf.cursor().col, 0);
        buf.move_to_line_end();
        assert_eq!(buf.cursor().col, 11);
    }

    #[test]
    fn move_to_doc_start_end() {
        let mut buf = EditorBuffer::from_text("first\nsecond\nthird");
        buf.set_cursor(1, 3);
        buf.move_to_doc_start();
        assert_eq!(buf.cursor(), Cursor::new(0, 0));
        buf.move_to_doc_end();
        assert_eq!(buf.cursor().line, 2);
    }

    #[test]
    fn word_movement() {
        let mut buf = EditorBuffer::from_text("hello world foo");
        buf.set_cursor(0, 0);
        buf.move_word_forward();
        assert_eq!(buf.cursor().col, 6); // start of "world"
        buf.move_word_forward();
        assert_eq!(buf.cursor().col, 12); // start of "foo"
        buf.move_word_backward();
        assert_eq!(buf.cursor().col, 6);
    }

    #[test]
    fn undo_redo() {
        let mut buf = EditorBuffer::new();
        // Insert with coalescing disabled by sleeping... actually let's just
        // set coalesce to 0 for testing
        buf.coalesce_ms = 0;
        buf.insert_char('a');
        buf.insert_char('b');
        assert_eq!(buf.text(), "ab");

        buf.undo();
        assert_eq!(buf.text(), "a");

        buf.redo();
        assert_eq!(buf.text(), "ab");
    }

    #[test]
    fn selection() {
        let mut buf = EditorBuffer::from_text("hello world");
        buf.set_cursor(0, 0);
        buf.start_selection();
        buf.set_cursor(0, 5);
        buf.extend_selection_to_cursor();
        let text = buf.selected_text();
        assert_eq!(text.as_deref(), Some("hello"));
    }

    #[test]
    fn yank_line() {
        let buf = EditorBuffer::from_text("first\nsecond\nthird");
        assert_eq!(buf.yank_line(), "first\n");
    }

    #[test]
    fn insert_text_multiline() {
        let mut buf = EditorBuffer::new();
        buf.insert_text("hello\nworld");
        assert_eq!(buf.text(), "hello\nworld");
        assert_eq!(buf.cursor(), Cursor::new(1, 5));
    }

    #[test]
    fn file_path() {
        let mut buf = EditorBuffer::new();
        assert!(buf.file_path().is_none());
        buf.set_file_path("test.md");
        assert_eq!(buf.file_path(), Some("test.md"));
    }
}
