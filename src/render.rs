//! GPU rendering pipeline for the hikki editor.
//!
//! Uses garasu for text rendering, madori for app framework,
//! and mojiban for markdown preview styling.

use garasu::GpuContext;
use glyphon::{Attrs, Buffer, Color, TextArea, TextBounds, Weight};
use ishou_tokens::FleetTheme;
use madori::render::{RenderCallback, RenderContext};
use mojiban::{MarkdownParser, TextWeight};

use crate::editor::EditorBuffer;
use crate::input::Mode;

/// Resolved paint palette, sourced from ishou design tokens via the
/// app's [`FleetTheme`]. No hand-authored hex lives at a paint site:
/// every color is resolved from `theme.resolve()` so a fleet theme
/// switch (`PlemeDark` ↔ `Bare`) propagates to the GPU renderer.
///
/// Only the colors actually painted today are held here (background,
/// foreground, dimmed foreground, status-bar mode colors); the former
/// dead-code accents were dropped.
struct Palette {
    /// Surface clear color (window background).
    bg: wgpu::Color,
    /// Primary editor text.
    fg: [f32; 4],
    /// Dimmed text — inactive line numbers.
    ///
    /// NOTE: held as a literal pending a faithful ishou
    /// "muted-foreground" / text-secondary token. The fleet palette
    /// has no exact match for this mid-grey (it sits between
    /// `snow_storm_0` and `polar_night_3`); mapping it to either
    /// shifts line-number legibility, so it stays explicit until
    /// ishou-tokens ships the role. See the spree notes.
    fg_dim: [f32; 4],
    /// Status-bar color in Normal mode.
    mode_normal: [f32; 4],
    /// Status-bar color in Insert mode.
    mode_insert: [f32; 4],
    /// Status-bar color in Visual mode.
    mode_visual: [f32; 4],
    /// Status-bar color in Command / Search mode.
    mode_command: [f32; 4],
}

impl Palette {
    /// Resolve every paint color from an ishou [`FleetTheme`]. The
    /// status-bar mode colors are read from the theme's ANSI-16 slots
    /// (frost-cyan / aurora-green / aurora-purple / aurora-yellow in
    /// xterm order) so they track the fleet palette, not a snapshot.
    fn from_theme(theme: FleetTheme) -> Self {
        let t = theme.resolve();
        Self {
            bg: wgpu_clear_from_hex(&t.background),
            fg: srgb_from_hex(&t.foreground),
            fg_dim: [0.616, 0.635, 0.659, 1.0],
            mode_normal: srgb_from_hex(&t.ansi_16[6]), // frost cyan
            mode_insert: srgb_from_hex(&t.ansi_16[2]), // aurora green
            mode_visual: srgb_from_hex(&t.ansi_16[5]), // aurora purple
            mode_command: srgb_from_hex(&t.ansi_16[3]), // aurora yellow
        }
    }

    /// Status-bar color for the current editor [`Mode`].
    fn mode(&self, mode: Mode) -> [f32; 4] {
        match mode {
            Mode::Normal => self.mode_normal,
            Mode::Insert => self.mode_insert,
            Mode::Visual => self.mode_visual,
            Mode::Command | Mode::Search => self.mode_command,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::from_theme(FleetTheme::default())
    }
}

/// Parse an `#RRGGBB` hex string from an ishou `ResolvedTheme` into a
/// straight-sRGB `[r, g, b, a]` (each 0.0–1.0) — the representation
/// glyphon + the wgpu surface clear consume. Unparseable input falls
/// back to opaque black so a malformed token can never panic the
/// render loop.
fn srgb_from_hex(hex: &str) -> [f32; 4] {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    let channel = |i: usize| -> f32 {
        f32::from(
            h.get(i..i + 2)
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .unwrap_or(0),
        ) / 255.0
    };
    [channel(0), channel(2), channel(4), 1.0]
}

/// `#RRGGBB` → `wgpu::Color` for the surface clear op, via the same
/// straight-sRGB path as [`srgb_from_hex`].
fn wgpu_clear_from_hex(hex: &str) -> wgpu::Color {
    let [r, g, b, a] = srgb_from_hex(hex);
    wgpu::Color {
        r: f64::from(r),
        g: f64::from(g),
        b: f64::from(b),
        a: f64::from(a),
    }
}

/// Visual state passed to the renderer each frame.
pub struct ViewState {
    /// The current editor buffer.
    pub buffer: EditorBuffer,
    /// Current editor mode.
    pub mode: Mode,
    /// Scroll offset in lines.
    pub scroll_offset: usize,
    /// Whether to show the preview panel.
    pub show_preview: bool,
    /// Whether to show the note list sidebar.
    pub show_note_list: bool,
    /// Note list items (title strings).
    pub note_list: Vec<String>,
    /// Selected index in note list.
    pub note_list_selected: usize,
    /// Status line message.
    pub status_message: String,
    /// Command/search bar text.
    pub command_text: String,
    /// Search query for highlighting.
    pub search_query: String,
    /// Backlinks for current note.
    pub backlinks: Vec<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            buffer: EditorBuffer::new(),
            mode: Mode::Normal,
            scroll_offset: 0,
            show_preview: false,
            show_note_list: true,
            note_list: Vec::new(),
            note_list_selected: 0,
            status_message: String::new(),
            command_text: String::new(),
            search_query: String::new(),
            backlinks: Vec::new(),
        }
    }
}

/// The main hikki renderer implementing madori's RenderCallback.
pub struct HikkiRenderer {
    /// Shared view state updated by the app logic.
    pub state: ViewState,
    font_size: f32,
    line_height: f32,
    /// Resolved ishou paint palette (selected by the fleet theme).
    palette: Palette,
    /// Used by `render_preview` for markdown-to-styled-spans conversion.
    #[allow(dead_code)]
    markdown_parser: MarkdownParser,
    width: u32,
    height: u32,
}

impl HikkiRenderer {
    #[must_use]
    pub fn new(font_size: f32, line_height: f32) -> Self {
        Self {
            state: ViewState::default(),
            font_size,
            line_height,
            palette: Palette::default(),
            markdown_parser: MarkdownParser::new(),
            width: 1280,
            height: 720,
        }
    }

    /// Select the fleet theme that drives every paint color. Wired
    /// from `AppearanceConfig::theme` so an operator theme switch
    /// reaches the renderer's palette.
    #[must_use]
    pub fn with_theme(mut self, theme: FleetTheme) -> Self {
        self.palette = Palette::from_theme(theme);
        self
    }

    /// Calculate how many visible lines fit in the editor area.
    #[must_use]
    pub fn visible_lines(&self) -> usize {
        let editor_height = self.height.saturating_sub(40) as f32; // minus status bar
        (editor_height / self.line_height).floor() as usize
    }

    /// Ensure the cursor is visible by adjusting scroll offset.
    pub fn ensure_cursor_visible(&mut self) {
        let cursor_line = self.state.buffer.cursor().line;
        let visible = self.visible_lines();
        if visible == 0 {
            return;
        }
        if cursor_line < self.state.scroll_offset {
            self.state.scroll_offset = cursor_line;
        } else if cursor_line >= self.state.scroll_offset + visible {
            self.state.scroll_offset = cursor_line + 1 - visible;
        }
    }

    /// Render the editor panel: line numbers + text content.
    fn render_editor(
        &self,
        text: &mut garasu::TextRenderer,
        buffers: &mut Vec<Buffer>,
        x_offset: f32,
        width: f32,
    ) {
        let gutter_width = 50.0;
        let _text_x = x_offset + gutter_width + 10.0;
        let text_width = width - gutter_width - 20.0;
        let visible = self.visible_lines();

        for i in 0..visible {
            let line_idx = self.state.scroll_offset + i;
            if line_idx >= self.state.buffer.line_count() {
                break;
            }

            // Line number
            let line_num = format!("{:>4}", line_idx + 1);

            let mut num_buf = text.create_buffer(
                &line_num,
                self.font_size,
                self.line_height,
            );
            num_buf.set_size(&mut text.font_system, Some(gutter_width), Some(self.line_height));
            num_buf.shape_until_scroll(&mut text.font_system, false);
            buffers.push(num_buf);

            // Line content
            let line_text = self.state.buffer.line_text(line_idx);
            let display_text = if line_text.is_empty() { " " } else { &line_text };

            let mut line_buf = text.create_buffer(
                display_text,
                self.font_size,
                self.line_height,
            );
            line_buf.set_size(&mut text.font_system, Some(text_width), Some(self.line_height));
            line_buf.shape_until_scroll(&mut text.font_system, false);
            buffers.push(line_buf);
        }
    }

    /// Build text areas from buffers for rendering.
    fn build_text_areas<'a>(
        &self,
        buffers: &'a [Buffer],
        x_offset: f32,
        width: f32,
    ) -> Vec<TextArea<'a>> {
        let gutter_width = 50.0;
        let text_x = x_offset + gutter_width + 10.0;
        let visible = self.visible_lines();
        let mut areas = Vec::new();

        let mut buf_idx = 0;
        for i in 0..visible {
            let line_idx = self.state.scroll_offset + i;
            if line_idx >= self.state.buffer.line_count() {
                break;
            }
            if buf_idx + 1 >= buffers.len() {
                break;
            }

            let y = i as f32 * self.line_height + 5.0;
            let is_current_line = line_idx == self.state.buffer.cursor().line;
            let line_num_color = if is_current_line {
                self.palette.fg
            } else {
                self.palette.fg_dim
            };

            // Line number area
            areas.push(TextArea {
                buffer: &buffers[buf_idx],
                left: x_offset + 5.0,
                top: y,
                scale: 1.0,
                bounds: TextBounds {
                    left: (x_offset + 5.0) as i32,
                    top: y as i32,
                    right: (x_offset + gutter_width) as i32,
                    bottom: (y + self.line_height) as i32,
                },
                default_color: to_glyphon_color(line_num_color),
                custom_glyphs: &[],
            });
            buf_idx += 1;

            // Line text area
            areas.push(TextArea {
                buffer: &buffers[buf_idx],
                left: text_x,
                top: y,
                scale: 1.0,
                bounds: TextBounds {
                    left: text_x as i32,
                    top: y as i32,
                    right: (x_offset + width) as i32,
                    bottom: (y + self.line_height) as i32,
                },
                default_color: to_glyphon_color(self.palette.fg),
                custom_glyphs: &[],
            });
            buf_idx += 1;
        }

        areas
    }

    /// Render the note list sidebar.
    #[allow(dead_code)]
    fn render_note_list(
        &self,
        text: &mut garasu::TextRenderer,
    ) -> (Vec<Buffer>, Vec<TextArea<'_>>) {
        let sidebar_width = 200.0;
        let mut buffers = Vec::new();

        // Title
        let mut title_buf = text.create_buffer(
            " Notes",
            self.font_size,
            self.line_height,
        );
        title_buf.set_size(&mut text.font_system, Some(sidebar_width), Some(self.line_height));
        title_buf.shape_until_scroll(&mut text.font_system, false);
        buffers.push(title_buf);

        // Note items
        let visible_count = (self.visible_lines()).min(self.state.note_list.len());
        for (i, title) in self.state.note_list.iter().take(visible_count).enumerate() {
            let prefix = if i == self.state.note_list_selected { "> " } else { "  " };
            let display = format!("{prefix}{title}");
            let mut item_buf = text.create_buffer(
                &display,
                self.font_size * 0.9,
                self.line_height,
            );
            item_buf.set_size(&mut text.font_system, Some(sidebar_width - 10.0), Some(self.line_height));
            item_buf.shape_until_scroll(&mut text.font_system, false);
            buffers.push(item_buf);
        }

        (buffers, Vec::new())
    }

    /// Render the markdown preview panel.
    #[allow(dead_code)]
    fn render_preview(
        &self,
        text: &mut garasu::TextRenderer,
        _x_offset: f32,
        width: f32,
    ) -> Vec<Buffer> {
        let content = self.state.buffer.text();
        let body = crate::notes::strip_front_matter(&content);
        let rich_lines = self.markdown_parser.parse(body);

        let mut buffers = Vec::new();
        let visible = self.visible_lines();

        for (_i, rich_line) in rich_lines.iter().enumerate().take(visible) {
            let plain = rich_line.plain_text();
            if plain.is_empty() {
                continue;
            }

            // Build spans with attributes for rich rendering
            let spans: Vec<(&str, Attrs<'_>)> = rich_line
                .spans
                .iter()
                .map(|span| {
                    let mut attrs = Attrs::new();
                    if span.style.weight == TextWeight::Bold {
                        attrs = attrs.weight(Weight::BOLD);
                    }
                    (span.text.as_str(), attrs)
                })
                .collect();

            let mut buf = text.create_rich_buffer(
                &spans,
                self.font_size,
                self.line_height,
            );
            buf.set_size(&mut text.font_system, Some(width - 20.0), Some(self.line_height));
            buf.shape_until_scroll(&mut text.font_system, false);
            buffers.push(buf);
        }

        buffers
    }

    /// Render the status bar at the bottom.
    fn render_status_bar(
        &self,
        text: &mut garasu::TextRenderer,
    ) -> Buffer {
        let mode_label = self.state.mode.label();
        let cursor = self.state.buffer.cursor();
        let modified = if self.state.buffer.is_modified() { "[+] " } else { "" };
        let filename = self.state.buffer.file_path().unwrap_or("untitled");

        let status = if !self.state.command_text.is_empty() {
            format!(":{}", self.state.command_text)
        } else if !self.state.search_query.is_empty()
            && self.state.mode == Mode::Search
        {
            format!("/{}", self.state.search_query)
        } else {
            format!(
                " {mode_label} | {modified}{filename} | {}:{} | {} lines",
                cursor.line + 1,
                cursor.col + 1,
                self.state.buffer.line_count(),
            )
        };

        let mut buf = text.create_buffer(
            &status,
            self.font_size * 0.9,
            self.line_height,
        );
        buf.set_size(
            &mut text.font_system,
            Some(self.width as f32),
            Some(self.line_height),
        );
        buf.shape_until_scroll(&mut text.font_system, false);
        buf
    }
}

fn to_glyphon_color(c: [f32; 4]) -> Color {
    Color::rgba(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (c[3] * 255.0) as u8,
    )
}

impl RenderCallback for HikkiRenderer {
    fn init(&mut self, _gpu: &GpuContext) {
        tracing::info!("hikki renderer initialized");
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    fn render(&mut self, ctx: &mut RenderContext<'_>) {
        // Calculate layout
        let sidebar_width = if self.state.show_note_list { 200.0_f32 } else { 0.0 };
        let main_width = ctx.width as f32 - sidebar_width;
        let (editor_width, _preview_width) = if self.state.show_preview {
            (main_width * 0.5, main_width * 0.5)
        } else {
            (main_width, 0.0)
        };

        // Build text buffers for all panels
        let mut all_buffers: Vec<Buffer> = Vec::new();
        let mut all_areas: Vec<TextArea<'_>> = Vec::new();

        // -- Editor panel --
        self.render_editor(ctx.text, &mut all_buffers, sidebar_width, editor_width);

        // -- Status bar --
        let status_buf = self.render_status_bar(ctx.text);
        all_buffers.push(status_buf);

        // Build text areas from the buffers we created
        let text_areas = self.build_text_areas(&all_buffers[..all_buffers.len() - 1], sidebar_width, editor_width);
        all_areas.extend(text_areas);

        // Status bar area
        let status_y = ctx.height as f32 - self.line_height - 5.0;
        let status_color = to_glyphon_color(self.palette.mode(self.state.mode));
        all_areas.push(TextArea {
            buffer: all_buffers.last().expect("status buffer should exist"),
            left: 5.0,
            top: status_y,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: status_y as i32,
                right: ctx.width as i32,
                bottom: ctx.height as i32,
            },
            default_color: status_color,
            custom_glyphs: &[],
        });

        // Prepare and render
        ctx.text
            .prepare(
                &ctx.gpu.device,
                &ctx.gpu.queue,
                ctx.width,
                ctx.height,
                all_areas,
            )
            .ok();

        let mut encoder = ctx.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("hikki_render"),
            },
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hikki_main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.palette.bg),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            ctx.text.render(&mut pass).ok();
        }

        ctx.gpu.queue.submit(std::iter::once(encoder.finish()));

        // Trim atlas to reclaim memory from old glyphs
        ctx.text.atlas.trim();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_glyphon_color_white() {
        let c = to_glyphon_color([1.0, 1.0, 1.0, 1.0]);
        // Color is opaque white
        assert_eq!(c, Color::rgba(255, 255, 255, 255));
    }

    #[test]
    fn to_glyphon_color_black() {
        let c = to_glyphon_color([0.0, 0.0, 0.0, 1.0]);
        assert_eq!(c, Color::rgba(0, 0, 0, 255));
    }

    #[test]
    fn mode_color_mapping() {
        let p = Palette::default();
        assert_eq!(p.mode(Mode::Normal), p.mode_normal);
        assert_eq!(p.mode(Mode::Insert), p.mode_insert);
        assert_eq!(p.mode(Mode::Visual), p.mode_visual);
        assert_eq!(p.mode(Mode::Command), p.mode_command);
        assert_eq!(p.mode(Mode::Search), p.mode_command);
    }

    #[test]
    fn palette_resolves_from_ishou_theme() {
        // PlemeDark background is Nord polar-night #2E3440 — the
        // canonical fleet dark surface. Proves the palette is sourced
        // from ishou tokens, not a hand-authored literal.
        let p = Palette::from_theme(FleetTheme::PlemeDark);
        assert!((p.bg.r - 46.0 / 255.0).abs() < 0.001);
        assert!((p.bg.g - 52.0 / 255.0).abs() < 0.001);
        assert!((p.bg.b - 64.0 / 255.0).abs() < 0.001);
        // Bare theme drops to a black surface + white foreground.
        let bare = Palette::from_theme(FleetTheme::Bare);
        assert_eq!(bare.bg.r, 0.0);
        assert_eq!(bare.fg, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn srgb_from_hex_parses_and_falls_back() {
        assert_eq!(srgb_from_hex("#000000"), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(srgb_from_hex("#FFFFFF"), [1.0, 1.0, 1.0, 1.0]);
        // Malformed input must not panic — falls back to black.
        assert_eq!(srgb_from_hex("nope"), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn view_state_defaults() {
        let state = ViewState::default();
        assert_eq!(state.mode, Mode::Normal);
        assert_eq!(state.scroll_offset, 0);
        assert!(!state.show_preview);
        assert!(state.show_note_list);
        assert!(state.note_list.is_empty());
    }

    #[test]
    fn renderer_visible_lines() {
        let renderer = HikkiRenderer::new(16.0, 24.0);
        // Default 720 height, minus 40 for status = 680, / 24 = 28
        let visible = renderer.visible_lines();
        assert!(visible > 0);
        assert!(visible < 50);
    }

    #[test]
    fn renderer_ensure_cursor_visible_below() {
        let mut renderer = HikkiRenderer::new(16.0, 24.0);
        renderer.state.buffer = EditorBuffer::from_text(
            &"line\n".repeat(100),
        );
        renderer.state.buffer.set_cursor(50, 0);
        renderer.ensure_cursor_visible();
        assert!(renderer.state.scroll_offset > 0);
        assert!(renderer.state.scroll_offset <= 50);
    }

    #[test]
    fn renderer_ensure_cursor_visible_above() {
        let mut renderer = HikkiRenderer::new(16.0, 24.0);
        renderer.state.buffer = EditorBuffer::from_text(
            &"line\n".repeat(100),
        );
        renderer.state.scroll_offset = 50;
        renderer.state.buffer.set_cursor(10, 0);
        renderer.ensure_cursor_visible();
        assert_eq!(renderer.state.scroll_offset, 10);
    }
}
