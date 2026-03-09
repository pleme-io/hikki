//! Keyboard input handling with vim-style modal editing.
//!
//! Modes: Normal, Insert, Visual, Command, Search.
//! Normal mode uses hjkl navigation, leader key sequences, etc.

use madori::event::{KeyCode, Modifiers};

/// Editor mode (vim-style modal editing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
    Search,
}

impl Mode {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Visual => "VISUAL",
            Self::Command => "COMMAND",
            Self::Search => "SEARCH",
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Actions the input handler can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // -- Cursor movement --
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordForward,
    MoveWordBackward,
    MoveLineStart,
    MoveLineEnd,
    MoveDocStart,
    MoveDocEnd,
    MoveHalfPageDown,
    MoveHalfPageUp,

    // -- Editing --
    InsertChar(char),
    InsertNewline,
    DeleteBack,
    DeleteForward,
    DeleteLine,
    YankLine,
    PasteBelow,

    // -- Line operations --
    OpenLineBelow,
    OpenLineAbove,

    // -- Mode changes --
    EnterInsertMode,
    EnterInsertModeAppend,
    EnterVisualMode,
    EnterCommandMode,
    EnterSearchMode,
    ExitToNormal,

    // -- Selection (visual mode) --
    ExtendSelection,
    YankSelection,
    DeleteSelection,

    // -- Undo/Redo --
    Undo,
    Redo,

    // -- Leader key sequences --
    FindFile,
    SearchVault,
    TogglePreview,
    ToggleNoteList,
    NewNote,
    FollowLink,
    ShowBacklinks,

    // -- Command/Search mode --
    CommandInput(char),
    CommandSubmit,
    CommandCancel,
    SearchInput(char),
    SearchSubmit,
    SearchCancel,

    // -- File operations --
    Save,

    // -- Note list navigation --
    NoteListNext,
    NoteListPrev,
    NoteListOpen,

    // -- Application --
    Quit,
    NoOp,
}

/// Input handler that maps key events to actions based on current mode.
pub struct InputHandler {
    mode: Mode,
    /// Whether the leader key (Space) was pressed and we're waiting for the next key.
    leader_pending: bool,
    /// Whether 'g' was pressed and we're waiting for the next key (gg, gd, etc.).
    g_pending: bool,
    /// Whether 'd' was pressed and we're waiting for the next key (dd).
    d_pending: bool,
    /// The command line buffer.
    command_buf: String,
    /// The search query buffer.
    search_buf: String,
}

impl InputHandler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            leader_pending: false,
            g_pending: false,
            d_pending: false,
            command_buf: String::new(),
            search_buf: String::new(),
        }
    }

    /// Get the current mode.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Set the mode directly.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.leader_pending = false;
        self.g_pending = false;
        self.d_pending = false;
        if mode == Mode::Command {
            self.command_buf.clear();
        }
        if mode == Mode::Search {
            self.search_buf.clear();
        }
    }

    /// Get the current command buffer contents.
    #[must_use]
    pub fn command_buf(&self) -> &str {
        &self.command_buf
    }

    /// Get the current search buffer contents.
    #[must_use]
    pub fn search_buf(&self) -> &str {
        &self.search_buf
    }

    /// Process a key press and return the resulting action.
    pub fn handle_key(&mut self, key: KeyCode, mods: Modifiers) -> Action {
        match self.mode {
            Mode::Normal => self.handle_normal(key, mods),
            Mode::Insert => self.handle_insert(key, mods),
            Mode::Visual => self.handle_visual(key, mods),
            Mode::Command => self.handle_command(key, mods),
            Mode::Search => self.handle_search(key, mods),
        }
    }

    fn handle_normal(&mut self, key: KeyCode, mods: Modifiers) -> Action {
        // Handle leader sequences
        if self.leader_pending {
            self.leader_pending = false;
            return match key {
                KeyCode::Char('f') => Action::FindFile,
                KeyCode::Char('s') => Action::SearchVault,
                KeyCode::Char('p') => Action::TogglePreview,
                KeyCode::Char('e') => Action::ToggleNoteList,
                KeyCode::Char('n') => Action::NewNote,
                KeyCode::Char('b') => Action::ShowBacklinks,
                _ => Action::NoOp,
            };
        }

        // Handle g-prefix sequences
        if self.g_pending {
            self.g_pending = false;
            return match key {
                KeyCode::Char('g') => Action::MoveDocStart,
                KeyCode::Char('d') => Action::FollowLink,
                _ => Action::NoOp,
            };
        }

        // Handle d-prefix sequences
        if self.d_pending {
            self.d_pending = false;
            return match key {
                KeyCode::Char('d') => Action::DeleteLine,
                _ => Action::NoOp,
            };
        }

        // Handle Ctrl modifiers
        if mods.ctrl {
            return match key {
                KeyCode::Char('d') => Action::MoveHalfPageDown,
                KeyCode::Char('u') => Action::MoveHalfPageUp,
                KeyCode::Char('r') => Action::Redo,
                KeyCode::Char('s') => Action::Save,
                _ => Action::NoOp,
            };
        }

        match key {
            // Movement
            KeyCode::Char('h') | KeyCode::Left => Action::MoveLeft,
            KeyCode::Char('j') | KeyCode::Down => Action::MoveDown,
            KeyCode::Char('k') | KeyCode::Up => Action::MoveUp,
            KeyCode::Char('l') | KeyCode::Right => Action::MoveRight,
            KeyCode::Char('w') => Action::MoveWordForward,
            KeyCode::Char('b') => Action::MoveWordBackward,
            KeyCode::Char('0') => Action::MoveLineStart,
            KeyCode::Char('$') => Action::MoveLineEnd,
            KeyCode::Char('G') => Action::MoveDocEnd,

            // Prefix keys
            KeyCode::Char('g') => {
                self.g_pending = true;
                Action::NoOp
            }
            KeyCode::Char('d') => {
                self.d_pending = true;
                Action::NoOp
            }

            // Mode transitions
            KeyCode::Char('i') => {
                self.set_mode(Mode::Insert);
                Action::EnterInsertMode
            }
            KeyCode::Char('a') => {
                self.set_mode(Mode::Insert);
                Action::EnterInsertModeAppend
            }
            KeyCode::Char('v') => {
                self.set_mode(Mode::Visual);
                Action::EnterVisualMode
            }
            KeyCode::Char(':') => {
                self.set_mode(Mode::Command);
                Action::EnterCommandMode
            }
            KeyCode::Char('/') => {
                self.set_mode(Mode::Search);
                Action::EnterSearchMode
            }

            // Line operations
            KeyCode::Char('o') => {
                self.set_mode(Mode::Insert);
                Action::OpenLineBelow
            }
            KeyCode::Char('O') => {
                self.set_mode(Mode::Insert);
                Action::OpenLineAbove
            }

            // Yank/paste
            KeyCode::Char('y') => Action::YankLine,
            KeyCode::Char('p') => Action::PasteBelow,

            // Undo
            KeyCode::Char('u') => Action::Undo,

            // Leader key
            KeyCode::Space => {
                self.leader_pending = true;
                Action::NoOp
            }

            _ => Action::NoOp,
        }
    }

    fn handle_insert(&mut self, key: KeyCode, mods: Modifiers) -> Action {
        if mods.ctrl {
            return match key {
                KeyCode::Char('s') => Action::Save,
                _ => Action::NoOp,
            };
        }

        match key {
            KeyCode::Escape => {
                self.set_mode(Mode::Normal);
                Action::ExitToNormal
            }
            KeyCode::Enter => Action::InsertNewline,
            KeyCode::Backspace => Action::DeleteBack,
            KeyCode::Delete => Action::DeleteForward,
            KeyCode::Left => Action::MoveLeft,
            KeyCode::Right => Action::MoveRight,
            KeyCode::Up => Action::MoveUp,
            KeyCode::Down => Action::MoveDown,
            KeyCode::Char(c) => Action::InsertChar(c),
            KeyCode::Tab => Action::InsertChar('\t'),
            _ => Action::NoOp,
        }
    }

    fn handle_visual(&mut self, key: KeyCode, mods: Modifiers) -> Action {
        let _ = mods;
        match key {
            KeyCode::Escape => {
                self.set_mode(Mode::Normal);
                Action::ExitToNormal
            }
            KeyCode::Char('h') | KeyCode::Left => Action::MoveLeft,
            KeyCode::Char('j') | KeyCode::Down => Action::MoveDown,
            KeyCode::Char('k') | KeyCode::Up => Action::MoveUp,
            KeyCode::Char('l') | KeyCode::Right => Action::MoveRight,
            KeyCode::Char('y') => {
                self.set_mode(Mode::Normal);
                Action::YankSelection
            }
            KeyCode::Char('d') => {
                self.set_mode(Mode::Normal);
                Action::DeleteSelection
            }
            _ => Action::NoOp,
        }
    }

    fn handle_command(&mut self, key: KeyCode, _mods: Modifiers) -> Action {
        match key {
            KeyCode::Escape => {
                self.set_mode(Mode::Normal);
                Action::CommandCancel
            }
            KeyCode::Enter => {
                self.set_mode(Mode::Normal);
                Action::CommandSubmit
            }
            KeyCode::Backspace => {
                self.command_buf.pop();
                if self.command_buf.is_empty() {
                    self.set_mode(Mode::Normal);
                    Action::CommandCancel
                } else {
                    Action::NoOp
                }
            }
            KeyCode::Char(c) => {
                self.command_buf.push(c);
                Action::CommandInput(c)
            }
            _ => Action::NoOp,
        }
    }

    fn handle_search(&mut self, key: KeyCode, _mods: Modifiers) -> Action {
        match key {
            KeyCode::Escape => {
                self.set_mode(Mode::Normal);
                Action::SearchCancel
            }
            KeyCode::Enter => {
                self.set_mode(Mode::Normal);
                Action::SearchSubmit
            }
            KeyCode::Backspace => {
                self.search_buf.pop();
                Action::NoOp
            }
            KeyCode::Char(c) => {
                self.search_buf.push(c);
                Action::SearchInput(c)
            }
            _ => Action::NoOp,
        }
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a command string (from : mode) into an Action.
#[must_use]
pub fn parse_command(cmd: &str) -> Action {
    let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
    match parts.first().copied() {
        Some("q" | "quit") => Action::Quit,
        Some("w" | "save") => Action::Save,
        Some("wq") => Action::Save, // save handled, quit follows
        Some("new") => Action::NewNote,
        Some("search") => Action::SearchVault,
        Some("e" | "open") => Action::FindFile,
        _ => Action::NoOp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_mods() -> Modifiers {
        Modifiers::default()
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Default::default()
        }
    }

    #[test]
    fn default_mode_is_normal() {
        let handler = InputHandler::new();
        assert_eq!(handler.mode(), Mode::Normal);
    }

    #[test]
    fn normal_hjkl_movement() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('h'), no_mods()), Action::MoveLeft);
        assert_eq!(handler.handle_key(KeyCode::Char('j'), no_mods()), Action::MoveDown);
        assert_eq!(handler.handle_key(KeyCode::Char('k'), no_mods()), Action::MoveUp);
        assert_eq!(handler.handle_key(KeyCode::Char('l'), no_mods()), Action::MoveRight);
    }

    #[test]
    fn normal_word_movement() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('w'), no_mods()), Action::MoveWordForward);
        assert_eq!(handler.handle_key(KeyCode::Char('b'), no_mods()), Action::MoveWordBackward);
    }

    #[test]
    fn normal_line_start_end() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('0'), no_mods()), Action::MoveLineStart);
        assert_eq!(handler.handle_key(KeyCode::Char('$'), no_mods()), Action::MoveLineEnd);
    }

    #[test]
    fn normal_gg_doc_start() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('g'), no_mods()), Action::NoOp);
        assert_eq!(handler.handle_key(KeyCode::Char('g'), no_mods()), Action::MoveDocStart);
    }

    #[test]
    fn normal_g_doc_end() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('G'), no_mods()), Action::MoveDocEnd);
    }

    #[test]
    fn normal_dd_delete_line() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('d'), no_mods()), Action::NoOp);
        assert_eq!(handler.handle_key(KeyCode::Char('d'), no_mods()), Action::DeleteLine);
    }

    #[test]
    fn enter_insert_mode() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('i'), no_mods()), Action::EnterInsertMode);
        assert_eq!(handler.mode(), Mode::Insert);
    }

    #[test]
    fn exit_insert_to_normal() {
        let mut handler = InputHandler::new();
        handler.set_mode(Mode::Insert);
        assert_eq!(handler.handle_key(KeyCode::Escape, no_mods()), Action::ExitToNormal);
        assert_eq!(handler.mode(), Mode::Normal);
    }

    #[test]
    fn insert_mode_typing() {
        let mut handler = InputHandler::new();
        handler.set_mode(Mode::Insert);
        assert_eq!(handler.handle_key(KeyCode::Char('a'), no_mods()), Action::InsertChar('a'));
        assert_eq!(handler.handle_key(KeyCode::Enter, no_mods()), Action::InsertNewline);
        assert_eq!(handler.handle_key(KeyCode::Backspace, no_mods()), Action::DeleteBack);
    }

    #[test]
    fn ctrl_d_u_half_page() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('d'), ctrl()), Action::MoveHalfPageDown);
        assert_eq!(handler.handle_key(KeyCode::Char('u'), ctrl()), Action::MoveHalfPageUp);
    }

    #[test]
    fn ctrl_s_save() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('s'), ctrl()), Action::Save);
    }

    #[test]
    fn undo_redo() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('u'), no_mods()), Action::Undo);
        assert_eq!(handler.handle_key(KeyCode::Char('r'), ctrl()), Action::Redo);
    }

    #[test]
    fn leader_key_sequences() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Space, no_mods()), Action::NoOp);
        assert_eq!(handler.handle_key(KeyCode::Char('f'), no_mods()), Action::FindFile);

        assert_eq!(handler.handle_key(KeyCode::Space, no_mods()), Action::NoOp);
        assert_eq!(handler.handle_key(KeyCode::Char('s'), no_mods()), Action::SearchVault);

        assert_eq!(handler.handle_key(KeyCode::Space, no_mods()), Action::NoOp);
        assert_eq!(handler.handle_key(KeyCode::Char('p'), no_mods()), Action::TogglePreview);
    }

    #[test]
    fn visual_mode() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('v'), no_mods()), Action::EnterVisualMode);
        assert_eq!(handler.mode(), Mode::Visual);

        assert_eq!(handler.handle_key(KeyCode::Char('y'), no_mods()), Action::YankSelection);
        assert_eq!(handler.mode(), Mode::Normal);
    }

    #[test]
    fn command_mode() {
        let mut handler = InputHandler::new();
        handler.handle_key(KeyCode::Char(':'), no_mods());
        assert_eq!(handler.mode(), Mode::Command);

        handler.handle_key(KeyCode::Char('q'), no_mods());
        assert_eq!(handler.command_buf(), "q");

        assert_eq!(handler.handle_key(KeyCode::Enter, no_mods()), Action::CommandSubmit);
        assert_eq!(handler.mode(), Mode::Normal);
    }

    #[test]
    fn search_mode() {
        let mut handler = InputHandler::new();
        handler.handle_key(KeyCode::Char('/'), no_mods());
        assert_eq!(handler.mode(), Mode::Search);

        handler.handle_key(KeyCode::Char('t'), no_mods());
        handler.handle_key(KeyCode::Char('e'), no_mods());
        handler.handle_key(KeyCode::Char('s'), no_mods());
        handler.handle_key(KeyCode::Char('t'), no_mods());
        assert_eq!(handler.search_buf(), "test");

        assert_eq!(handler.handle_key(KeyCode::Enter, no_mods()), Action::SearchSubmit);
    }

    #[test]
    fn parse_command_quit() {
        assert_eq!(parse_command("q"), Action::Quit);
        assert_eq!(parse_command("quit"), Action::Quit);
    }

    #[test]
    fn parse_command_save() {
        assert_eq!(parse_command("w"), Action::Save);
        assert_eq!(parse_command("save"), Action::Save);
    }

    #[test]
    fn parse_command_new() {
        assert_eq!(parse_command("new My Note"), Action::NewNote);
    }

    #[test]
    fn mode_labels() {
        assert_eq!(Mode::Normal.label(), "NORMAL");
        assert_eq!(Mode::Insert.label(), "INSERT");
        assert_eq!(Mode::Visual.label(), "VISUAL");
        assert_eq!(Mode::Command.label(), "COMMAND");
        assert_eq!(Mode::Search.label(), "SEARCH");
    }

    #[test]
    fn gd_follow_link() {
        let mut handler = InputHandler::new();
        handler.handle_key(KeyCode::Char('g'), no_mods());
        assert_eq!(handler.handle_key(KeyCode::Char('d'), no_mods()), Action::FollowLink);
    }

    #[test]
    fn open_line_enters_insert() {
        let mut handler = InputHandler::new();
        assert_eq!(handler.handle_key(KeyCode::Char('o'), no_mods()), Action::OpenLineBelow);
        assert_eq!(handler.mode(), Mode::Insert);
    }
}
