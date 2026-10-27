//! Inline diagrams: Mermaid source that an agent prints between
//! `<wezmux_diagram>` tags is drawn in place of its text.
//!
//! The grid belongs to the agent's TUI, so the diagram cannot claim rows of
//! its own: it is fitted into the rectangle its source text occupies, and the
//! text of those rows is not painted. Until the closing tag arrives (or when
//! the source does not parse) the text shows as-is.
//!
//! Detection runs over the pane's text, so it works for any agent that leaves
//! the tag lines alone (Claude Code and Codex both do; Claude Code strips
//! ```` ```mermaid ```` fences and Codex turns them into box art). Codex's own
//! "exceeds the terminal renderer's size limits" fallback is picked up too.
//!
//! Rendering (mermaid-rs-renderer → SVG → resvg) happens off the GUI thread at
//! the exact pixel size of the target rectangle, in the terminal's font and
//! colors, and is redone when that size or the palette changes.
//!
//! LaTeX math (`math.rs`) goes through the same scan, cache and render
//! thread: `$$` display math as a block like a diagram, `$…$` inline math
//! as a span of cells within a row.

use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::selection::SelectionRange;
use crate::termwindow::math::{self, PlacedInline};
use crate::termwindow::render::paint::AllowImage;
use crate::termwindow::TermWindowNotif;
use ::window::bitmaps::{TextureCoord, TextureRect, TextureSize};
use ::window::WindowOps;
use mermaid_rs_renderer::{RenderOptions, Theme};
use mux::pane::PaneId;
use mux::tab::PositionedPane;
use promise::spawn::{spawn, spawn_into_new_thread};
use resvg::{tiny_skia, usvg};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};
use termwiz::cell::unicode_column_width;
use termwiz::image::{ImageData, ImageDataType};
use termwiz::surface::SequenceNo;
use wezterm_font::locator::FontDataHandle;
use wezterm_term::color::{ColorPalette, SrgbaTuple};
use wezterm_term::StableRowIndex;

const OPEN_TAG: &str = "<wezmux_diagram";
const CLOSE_TAG: &str = "</wezmux_diagram>";
const CODEX_FALLBACK: &str = "This Mermaid diagram exceeds the terminal renderer";
/// Agent TUIs prefix the first line of a message with a bullet.
const BULLETS: &[char] = &['⏺', '•', '●'];
const MAX_BLOCK_ROWS: usize = 200;
/// Below this the text would be too small to read, so the source is kept.
const MIN_SCALE: f32 = 0.5;
const MAX_PIXELS: u32 = 4096;
/// Roomy enough for every formula on screen, since inline math adds many
/// small renders and a full cache is dropped as a whole.
const MAX_CACHED_RENDERS: usize = 512;

/// One terminal row of text, with the number of physical rows it spans
/// when the terminal soft-wrapped it.
pub struct TextLine {
    pub row: StableRowIndex,
    pub rows: usize,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderKind {
    Mermaid,
    DisplayMath,
    InlineMath,
}

fn source_hash(kind: RenderKind, source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    source.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiagramBlock {
    pub kind: RenderKind,
    /// Rows covered, tags included.
    pub rows: Range<StableRowIndex>,
    /// Column of the opening tag; the diagram is drawn from here.
    pub col: usize,
    pub source: Arc<str>,
    pub source_hash: u64,
}

impl DiagramBlock {
    fn new(kind: RenderKind, rows: Range<StableRowIndex>, col: usize, source: String) -> Self {
        Self {
            kind,
            rows,
            col,
            source_hash: source_hash(kind, &source),
            source: source.into(),
        }
    }
}

/// `$…$` math found on one row.
#[derive(Clone, Debug, PartialEq)]
pub struct InlineMath {
    pub row: StableRowIndex,
    pub cols: Range<usize>,
    pub source: Arc<str>,
    pub source_hash: u64,
}

/// Splits off the indentation or message bullet that agent TUIs put before
/// a line, returning its width in columns and the rest of the line.
pub fn split_indent(text: &str) -> (usize, &str) {
    let body = text.trim_start_matches(|c: char| c.is_whitespace() || BULLETS.contains(&c));
    (
        unicode_column_width(&text[..text.len() - body.len()], None),
        body,
    )
}

/// Returns the column of the opening tag when `text` is a line holding only
/// that tag (after indentation or a message bullet), so that prose which
/// merely mentions the tag is left alone.
fn opening_tag_col(text: &str) -> Option<usize> {
    let idx = text.find(OPEN_TAG)?;
    let (prefix, tag) = text.split_at(idx);
    if !prefix
        .trim_start_matches(|c: char| c.is_whitespace() || BULLETS.contains(&c))
        .is_empty()
    {
        return None;
    }
    let tag = tag.trim_end();
    let attrs = tag.strip_prefix(OPEN_TAG)?.strip_suffix('>')?;
    if !(attrs.is_empty() || attrs.starts_with(char::is_whitespace)) {
        return None;
    }
    if let Some(lang) = attrs.split("lang=").nth(1) {
        let lang = lang.trim().trim_matches(|c| c == '"' || c == '\'');
        if !lang.eq_ignore_ascii_case("mermaid") {
            return None;
        }
    }
    Some(unicode_column_width(prefix, None))
}

fn block_rows(lines: &[TextLine], first: usize, last: usize) -> Range<StableRowIndex> {
    lines[first].row..lines[last].row + lines[last].rows as StableRowIndex
}

/// Finds Mermaid blocks, and `$$` math blocks when `math` is set.
pub fn find_blocks(lines: &[TextLine], math: bool) -> Vec<DiagramBlock> {
    let mut blocks = vec![];
    let mut i = 0;
    while i < lines.len() {
        let text = &lines[i].text;
        let window = i + 1..lines.len().min(i + 1 + MAX_BLOCK_ROWS);

        if let Some(col) = opening_tag_col(text) {
            if let Some(end) = window.clone().find(|&j| lines[j].text.trim() == CLOSE_TAG) {
                let source = lines[i + 1..end]
                    .iter()
                    .map(|l| l.text.trim_end())
                    .collect::<Vec<_>>()
                    .join("\n");
                blocks.push(DiagramBlock::new(
                    RenderKind::Mermaid,
                    block_rows(lines, i, end),
                    col,
                    source,
                ));
                i = end + 1;
                continue;
            }
        } else if let Some((end, col, source)) =
            math.then(|| math::display_math_at(lines, i)).flatten()
        {
            blocks.push(DiagramBlock::new(
                RenderKind::DisplayMath,
                block_rows(lines, i, end),
                col,
                source,
            ));
            i = end + 1;
            continue;
        } else if text.contains(CODEX_FALLBACK) {
            let (col, _) = split_indent(text);
            let end = window
                .take_while(|&j| !lines[j].text.trim().is_empty())
                .last();
            if let Some(end) = end {
                let source = lines[i + 1..=end]
                    .iter()
                    .map(|l| l.text.trim_end())
                    .collect::<Vec<_>>()
                    .join("\n");
                blocks.push(DiagramBlock::new(
                    RenderKind::Mermaid,
                    block_rows(lines, i, end),
                    col,
                    source,
                ));
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    blocks
}

/// What the diagram is drawn with: the terminal's colors and font.
pub struct DiagramStyle {
    pub foreground: (u8, u8, u8),
    background: (u8, u8, u8),
    ansi: [(u8, u8, u8); 16],
    font_family: String,
    /// The terminal's font file, which need not be installed system-wide
    /// (the bundled JetBrains Mono isn't).
    font: Option<FontDataHandle>,
    /// Font size in device pixels, so that at scale 1.0 diagram text
    /// matches the terminal text.
    pub font_px: u32,
}

fn rgb(c: SrgbaTuple) -> (u8, u8, u8) {
    let (r, g, b, _) = c.to_srgb_u8();
    (r, g, b)
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

impl DiagramStyle {
    pub fn new(
        palette: &ColorPalette,
        font_family: &str,
        font: Option<FontDataHandle>,
        font_px: f32,
    ) -> Self {
        let mut ansi = [(0, 0, 0); 16];
        for (i, c) in ansi.iter_mut().enumerate() {
            *c = rgb(palette.colors.0[i]);
        }
        Self {
            foreground: rgb(palette.foreground),
            background: rgb(palette.background),
            ansi,
            font_family: font_family.to_string(),
            font,
            font_px: font_px.round() as u32,
        }
    }

    fn hash_value(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.foreground.hash(&mut hasher);
        self.background.hash(&mut hasher);
        self.ansi.hash(&mut hasher);
        self.font_family.hash(&mut hasher);
        self.font.hash(&mut hasher);
        self.font_px.hash(&mut hasher);
        hasher.finish()
    }

    /// `color` laid over the background at `alpha`, as an opaque color, so
    /// the diagram reads as tints of the terminal rather than pasted-on fills.
    fn tint(&self, color: (u8, u8, u8), alpha: f32) -> String {
        let mix = |c: u8, b: u8| (b as f32 + (c as f32 - b as f32) * alpha).round() as u8;
        let (r, g, b) = color;
        let (br, bg, bb) = self.background;
        hex((mix(r, br), mix(g, bg), mix(b, bb)))
    }

    fn fg(&self, alpha: f32) -> String {
        self.tint(self.foreground, alpha)
    }

    fn theme(&self) -> Theme {
        let text = hex(self.foreground);
        let accents: Vec<String> = [4, 2, 5, 3, 6, 1, 12, 10, 13, 11, 14, 9]
            .iter()
            .map(|&i| hex(self.ansi[i]))
            .collect();
        let accent = |i: usize| accents[i % accents.len()].clone();

        Theme {
            font_family: format!("\"{}\", ui-monospace, Menlo, monospace", self.font_family),
            font_size: self.font_px as f32,
            primary_color: self.fg(0.06),
            primary_text_color: text.clone(),
            primary_border_color: self.fg(0.45),
            line_color: self.fg(0.55),
            secondary_color: self.fg(0.10),
            tertiary_color: self.fg(0.03),
            edge_label_background: hex(self.background),
            cluster_background: self.fg(0.03),
            cluster_border: self.fg(0.22),
            background: "none".to_string(),
            sequence_actor_fill: self.fg(0.06),
            sequence_actor_border: self.fg(0.45),
            sequence_actor_line: self.fg(0.30),
            sequence_note_fill: self.tint(self.ansi[3], 0.14),
            sequence_note_border: self.tint(self.ansi[3], 0.55),
            sequence_activation_fill: self.fg(0.12),
            sequence_activation_border: self.fg(0.45),
            text_color: text.clone(),
            git_colors: std::array::from_fn(accent),
            git_inv_colors: std::array::from_fn(|_| hex(self.background)),
            git_branch_label_colors: std::array::from_fn(|_| hex(self.background)),
            git_commit_label_color: text.clone(),
            git_commit_label_background: self.fg(0.10),
            git_tag_label_color: text.clone(),
            git_tag_label_background: self.fg(0.10),
            git_tag_label_border: self.fg(0.40),
            pie_colors: std::array::from_fn(accent),
            pie_title_text_color: text.clone(),
            pie_section_text_color: hex(self.background),
            pie_legend_text_color: text,
            pie_stroke_color: hex(self.background),
            pie_outer_stroke_color: self.fg(0.30),
            pie_title_text_size: self.font_px as f32 * 1.2,
            pie_section_text_size: self.font_px as f32,
            pie_legend_text_size: self.font_px as f32,
            pie_opacity: 0.9,
            ..Theme::modern()
        }
    }
}

#[derive(Clone)]
pub struct Rendered {
    pub image: Arc<ImageData>,
    pub width: u32,
    pub height: u32,
    /// Distance from the top to the math baseline, in pixels; zero for
    /// diagrams, which have none.
    pub baseline: f32,
}

impl Rendered {
    #[cfg(test)]
    pub fn blank() -> Self {
        Self {
            image: Arc::new(ImageData::with_data(ImageDataType::new_single_frame(
                1,
                1,
                vec![0; 4],
            ))),
            width: 1,
            height: 1,
            baseline: 0.,
        }
    }
}

/// Rasterizes `tree` at `scale` into straight-alpha RGBA.
pub fn rasterize(tree: &usvg::Tree, scale: f32) -> anyhow::Result<Rendered> {
    let size = tree.size();
    let width = ((size.width() * scale).ceil() as u32).clamp(1, MAX_PIXELS);
    let height = ((size.height() * scale).ceil() as u32).clamp(1, MAX_PIXELS);
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("invalid pixmap size {width}x{height}"))?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let data = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Ok(Rendered {
        image: Arc::new(ImageData::with_data(ImageDataType::new_single_frame(
            width, height, data,
        ))),
        width,
        height,
        baseline: 0.,
    })
}

static SYSTEM_FONTS: LazyLock<usvg::fontdb::Database> = LazyLock::new(|| {
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    db
});

/// The system fonts plus the terminal's font, rebuilt when that font changes.
static FONTS: Mutex<Option<(FontDataHandle, Arc<usvg::fontdb::Database>)>> = Mutex::new(None);

fn fonts_with(font: Option<&FontDataHandle>) -> Arc<usvg::fontdb::Database> {
    let Some(font) = font else {
        return Arc::new(SYSTEM_FONTS.clone());
    };
    let mut fonts = FONTS.lock().unwrap();
    if let Some((handle, db)) = fonts.as_ref() {
        if handle == font {
            return Arc::clone(db);
        }
    }
    let mut db = SYSTEM_FONTS.clone();
    match font.source.load_data() {
        Ok(data) => db.load_font_data(data.into_owned()),
        Err(err) => log::warn!("diagram font {}: {err:#}", font.diagnostic_string()),
    }
    let db = Arc::new(db);
    *fonts = Some((font.clone(), Arc::clone(&db)));
    db
}

/// Lays out `source` and rasterizes it to fit `box_w` x `box_h` device
/// pixels, never above the terminal's text size.
pub fn render_mermaid(
    source: &str,
    style: &DiagramStyle,
    box_w: u32,
    box_h: u32,
) -> anyhow::Result<Rendered> {
    let usvg_options = usvg::Options {
        fontdb: fonts_with(style.font.as_ref()),
        ..usvg::Options::default()
    };
    let layout = |options: RenderOptions| -> anyhow::Result<(usvg::Tree, f32)> {
        let svg = mermaid_rs_renderer::render_with_options(source, options)?;
        let tree = usvg::Tree::from_str(&svg, &usvg_options)?;
        let size = tree.size();
        let scale = (box_w as f32 / size.width())
            .min(box_h as f32 / size.height())
            .min(1.0);
        Ok((tree, scale))
    };

    let options = RenderOptions {
        theme: style.theme(),
        ..RenderOptions::default()
    };
    // The natural layout reads best; only when it is too big for the rows is
    // it re-laid out toward the shape of the box.
    let (mut tree, mut scale) = layout(options.clone())?;
    if scale < 1.0 {
        let fitted = layout(options.with_preferred_aspect_ratio_parts(box_w as f32, box_h as f32))?;
        if fitted.1 > scale {
            (tree, scale) = fitted;
        }
    }
    let size = tree.size();
    anyhow::ensure!(
        scale >= MIN_SCALE,
        "diagram needs {:.0}x{:.0}px, has {box_w}x{box_h}",
        size.width(),
        size.height()
    );
    rasterize(&tree, scale)
}

fn render(
    kind: RenderKind,
    source: &str,
    style: &DiagramStyle,
    box_w: u32,
    box_h: u32,
) -> anyhow::Result<Rendered> {
    match kind {
        RenderKind::Mermaid => render_mermaid(source, style, box_w, box_h),
        RenderKind::DisplayMath => math::render_math(source, true, style, box_w, box_h),
        RenderKind::InlineMath => math::render_math(source, false, style, box_w, box_h),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RenderKey {
    source_hash: u64,
    style_hash: u64,
    box_w: u32,
    box_h: u32,
}

struct PaneScan {
    seqno: SequenceNo,
    range: Range<StableRowIndex>,
    math: bool,
    blocks: Arc<Vec<DiagramBlock>>,
    inline: Arc<Vec<InlineMath>>,
}

#[derive(Default)]
pub struct DiagramState {
    scans: HashMap<PaneId, PaneScan>,
    renders: HashMap<RenderKey, Option<Rendered>>,
    /// Latest finished render of each source, shown while a render at a new
    /// size is in flight so resizing doesn't flash the source text.
    latest: HashMap<u64, Rendered>,
    in_flight: HashSet<u64>,
    pub collapsed: math::CollapsedLines,
    /// The inline math last painted in each pane, for mapping the mouse.
    painted_inline: HashMap<PaneId, Vec<PlacedInline>>,
}

impl DiagramState {
    pub fn set_painted_inline(&mut self, pane_id: PaneId, inline: Vec<PlacedInline>) {
        self.painted_inline.insert(pane_id, inline);
    }

    /// The grid column under column `col` of `row` as painted, which differs
    /// from `col` right of inline math (see [`math::grid_col`]).
    pub fn grid_col(&self, pane_id: PaneId, row: StableRowIndex, col: usize) -> usize {
        let Some(inline) = self.painted_inline.get(&pane_id) else {
            return col;
        };
        let spans: Vec<_> = inline.iter().filter(|m| m.row == row).collect();
        math::grid_col(&spans, col)
    }

    fn finish(&mut self, key: RenderKey, result: anyhow::Result<Rendered>) {
        self.in_flight.remove(&key.source_hash);
        if self.renders.len() >= MAX_CACHED_RENDERS {
            self.renders.clear();
            self.latest.clear();
        }
        let rendered = match result {
            Ok(rendered) => {
                self.latest.insert(key.source_hash, rendered.clone());
                Some(rendered)
            }
            Err(err) => {
                log::debug!("inline diagram not rendered: {err:#}");
                self.latest.remove(&key.source_hash);
                None
            }
        };
        self.renders.insert(key, rendered);
    }
}

/// A diagram ready to draw over a pane.
pub struct PlacedDiagram {
    pub rows: Range<StableRowIndex>,
    pub col: usize,
    pub box_w: f32,
    pub box_h: f32,
    pub rendered: Rendered,
}

/// What is drawn over a pane instead of its text: diagram and display math
/// blocks over whole rows, inline math within rows (sorted by row, then
/// column).
#[derive(Default)]
pub struct PaneOverlays {
    pub blocks: Vec<PlacedDiagram>,
    pub inline: Vec<PlacedInline>,
}

impl PaneOverlays {
    /// The inline math of `row`, in column order.
    pub fn inline_on(&self, row: StableRowIndex) -> Vec<&PlacedInline> {
        self.inline.iter().filter(|m| m.row == row).collect()
    }
}

/// The current render for `key`, or the latest one at another size while
/// a render at this size is requested.
enum Lookup {
    Ready(Rendered),
    Failed,
    Missing,
}

impl crate::TermWindow {
    fn scan_pane_for_diagrams(
        &mut self,
        pos: &PositionedPane,
        viewport: &Range<StableRowIndex>,
    ) -> (Arc<Vec<DiagramBlock>>, Arc<Vec<InlineMath>>) {
        let pane_id = pos.pane.pane_id();
        let seqno = pos.pane.get_current_seqno();
        // Math is left alone in full-screen apps such as editors, where the
        // source is being worked on, unless an agent runs there (Codex draws
        // on the alternate screen).
        let math = !pos.pane.is_alt_screen_active()
            || mux::Mux::try_get().is_some_and(|mux| mux.agent_status_for_pane(pane_id).is_some());
        // Look past the viewport so blocks that are partly scrolled out are
        // still found whole.
        let margin = MAX_BLOCK_ROWS as StableRowIndex;
        let range = viewport.start - margin..viewport.end + margin;
        if let Some(scan) = self.diagrams.scans.get(&pane_id) {
            if scan.seqno == seqno && scan.range == range && scan.math == math {
                return (Arc::clone(&scan.blocks), Arc::clone(&scan.inline));
            }
        }

        let (first_row, lines) = pos.pane.get_lines(range.clone());
        let mut text_lines: Vec<TextLine> = vec![];
        let mut inline = vec![];
        let mut continues = false;
        for (idx, line) in lines.iter().enumerate() {
            let row = first_row + idx as StableRowIndex;
            let text = line.as_str();
            if math && text.contains('$') {
                let chars: Vec<(usize, char)> = line
                    .visible_cells()
                    .flat_map(|cell| {
                        let col = cell.cell_index();
                        cell.str()
                            .chars()
                            .map(move |c| (col, c))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                inline.extend(
                    math::find_inline_math(&chars)
                        .into_iter()
                        .map(|span| InlineMath {
                            row,
                            cols: span.cols,
                            source_hash: source_hash(RenderKind::InlineMath, &span.source),
                            source: span.source.into(),
                        }),
                );
            }
            match text_lines.last_mut() {
                Some(prev) if continues => {
                    prev.text.push_str(text.trim_end());
                    prev.rows += 1;
                }
                _ => text_lines.push(TextLine {
                    row,
                    rows: 1,
                    text: text.into_owned(),
                }),
            }
            continues = line.last_cell_was_wrapped();
        }

        let blocks = find_blocks(&text_lines, math);
        // A `$` inside a block's source is the block's business.
        inline.retain(|m: &InlineMath| !blocks.iter().any(|b| b.rows.contains(&m.row)));
        let blocks = Arc::new(blocks);
        let inline = Arc::new(inline);
        self.diagrams.scans.insert(
            pane_id,
            PaneScan {
                seqno,
                range,
                math,
                blocks: Arc::clone(&blocks),
                inline: Arc::clone(&inline),
            },
        );
        (blocks, inline)
    }

    fn diagram_style(&self, palette: &ColorPalette) -> Arc<DiagramStyle> {
        let (font_family, font, font_px) = match self
            .fonts
            .default_font()
            .ok()
            .and_then(|f| f.primary_face())
        {
            Some((family, handle, px)) => (family, Some(handle), px as f32),
            None => (
                String::new(),
                None,
                self.config.font_size as f32 * self.dimensions.dpi as f32 / 72.0,
            ),
        };
        Arc::new(DiagramStyle::new(palette, &font_family, font, font_px))
    }

    fn lookup_render(
        &mut self,
        kind: RenderKind,
        key: RenderKey,
        source: &Arc<str>,
        style: &Arc<DiagramStyle>,
    ) -> Option<Rendered> {
        let lookup = match self.diagrams.renders.get(&key) {
            Some(Some(rendered)) => Lookup::Ready(rendered.clone()),
            Some(None) => Lookup::Failed,
            None => Lookup::Missing,
        };
        match lookup {
            Lookup::Ready(rendered) => Some(rendered),
            Lookup::Failed => None,
            Lookup::Missing => {
                self.request_diagram_render(kind, key, Arc::clone(source), style);
                self.diagrams.latest.get(&key.source_hash).cloned()
            }
        }
    }

    /// Finds the diagram and math blocks and the inline math visible in
    /// `viewport`, starts renders for those not ready at the current size,
    /// and returns the drawable ones. Inline math on `cursor_row` stays
    /// text, since that row may be a prompt being typed into, and so does
    /// anything the selection touches, so that what is selected is what is
    /// copied.
    pub fn diagrams_for_pane(
        &mut self,
        pos: &PositionedPane,
        viewport: &Range<StableRowIndex>,
        palette: &ColorPalette,
        cursor_row: StableRowIndex,
        selection: Option<(&SelectionRange, bool)>,
    ) -> PaneOverlays {
        let selected = |row: StableRowIndex, cols: &Range<usize>| {
            selection.is_some_and(|(sel, rectangular)| {
                let sel = sel.cols_for_row(row, rectangular);
                sel.start < cols.end && cols.start < sel.end
            })
        };
        let (blocks, inline) = self.scan_pane_for_diagrams(pos, viewport);
        if blocks.is_empty() && inline.is_empty() {
            return PaneOverlays::default();
        }

        let cell_w = self.render_metrics.cell_size.width as f32;
        let cell_h = self.render_metrics.cell_size.height as f32;
        let style = self.diagram_style(palette);
        let style_hash = style.hash_value();
        let cols = pos.pane.get_dimensions().cols;

        let mut overlays = PaneOverlays::default();
        for block in blocks.iter() {
            if block.rows.end <= viewport.start
                || block.rows.start >= viewport.end
                || block
                    .rows
                    .clone()
                    .any(|row| selected(row, &(0..usize::MAX)))
            {
                continue;
            }
            let box_w = cols.saturating_sub(block.col) as f32 * cell_w;
            let box_h = (block.rows.end - block.rows.start) as f32 * cell_h;
            let key = RenderKey {
                source_hash: block.source_hash,
                style_hash,
                box_w: box_w as u32,
                box_h: box_h as u32,
            };
            if let Some(rendered) = self.lookup_render(block.kind, key, &block.source, &style) {
                overlays.blocks.push(PlacedDiagram {
                    rows: block.rows.clone(),
                    col: block.col,
                    box_w,
                    box_h,
                    rendered,
                });
            }
        }

        for span in inline.iter() {
            if !viewport.contains(&span.row)
                || span.row == cursor_row
                || selected(span.row, &span.cols)
            {
                continue;
            }
            let box_w = span.cols.len() as f32 * cell_w;
            let key = RenderKey {
                source_hash: span.source_hash,
                style_hash,
                box_w: box_w as u32,
                box_h: cell_h as u32,
            };
            let Some(rendered) =
                self.lookup_render(RenderKind::InlineMath, key, &span.source, &style)
            else {
                continue;
            };
            // A stale render from another size is shrunk to fit, never grown.
            let fit = (box_w / rendered.width as f32)
                .min(cell_h / rendered.height as f32)
                .min(1.0);
            let width = rendered.width as f32 * fit;
            overlays.inline.push(PlacedInline {
                row: span.row,
                cols: span.cols.clone(),
                cells: ((width / cell_w).ceil() as usize).clamp(1, span.cols.len()),
                draw_col: 0,
                width,
                height: rendered.height as f32 * fit,
                baseline: rendered.baseline * fit,
                rendered,
            });
        }
        // Each formula is drawn where its row puts it once collapsed.
        for i in 0..overlays.inline.len() {
            let row = overlays.inline[i].row;
            let start = overlays.inline[i].cols.start;
            let draw_col = math::collapsed_col(&overlays.inline_on(row), start);
            overlays.inline[i].draw_col = draw_col;
        }
        overlays
    }

    fn request_diagram_render(
        &mut self,
        kind: RenderKind,
        key: RenderKey,
        source: Arc<str>,
        style: &Arc<DiagramStyle>,
    ) {
        // One render per source at a time; a size that changed meanwhile is
        // picked up by the next paint.
        if !self.diagrams.in_flight.insert(key.source_hash) {
            return;
        }
        let Some(window) = self.window.clone() else {
            self.diagrams.in_flight.remove(&key.source_hash);
            return;
        };
        let style = Arc::clone(style);
        spawn(async move {
            let result = spawn_into_new_thread(move || {
                Ok(render(kind, &source, &style, key.box_w, key.box_h))
            })
            .await
            .and_then(|r| r);
            window.notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                term_window.diagrams.finish(key, result);
                if let Some(window) = term_window.window.as_ref() {
                    window.invalidate();
                }
            })));
        })
        .detach();
    }

    /// Draws `diagram` left-aligned with its tag and vertically centered in
    /// its rows, clipped to the pane's viewport.
    pub fn paint_diagram(
        &self,
        layers: &mut TripleLayerQuadAllocator,
        diagram: &PlacedDiagram,
        viewport_top: StableRowIndex,
        viewport_rows: usize,
        pane_top_px: f32,
        left_pixel_x: f32,
    ) -> anyhow::Result<()> {
        let cell_w = self.render_metrics.cell_size.width as f32;
        let cell_h = self.render_metrics.cell_size.height as f32;
        let rendered = &diagram.rendered;

        // A stale render from another size is shrunk to fit, never grown.
        let fit = (diagram.box_w / rendered.width as f32)
            .min(diagram.box_h / rendered.height as f32)
            .min(1.0);
        let width = rendered.width as f32 * fit;
        let height = rendered.height as f32 * fit;

        let x0 = left_pixel_x + diagram.col as f32 * cell_w;
        let box_top = pane_top_px + (diagram.rows.start - viewport_top) as f32 * cell_h;
        let y0 = box_top + ((diagram.box_h - height) / 2.0).floor();
        let clip = pane_top_px..pane_top_px + viewport_rows as f32 * cell_h;
        self.paint_rendered(layers, rendered, (x0, y0), (width, height), clip)
    }

    /// Draws inline math on its row's baseline, centered in the cells the
    /// collapsed row leaves for it.
    pub fn paint_inline_math(
        &self,
        layers: &mut TripleLayerQuadAllocator,
        math: &PlacedInline,
        viewport_top: StableRowIndex,
        pane_top_px: f32,
        left_pixel_x: f32,
    ) -> anyhow::Result<()> {
        let cell_w = self.render_metrics.cell_size.width as f32;
        let cell_h = self.render_metrics.cell_size.height as f32;
        let row_top = pane_top_px + (math.row - viewport_top) as f32 * cell_h;
        let baseline = row_top + cell_h + self.render_metrics.descender.get() as f32;

        let x0 = left_pixel_x
            + math.draw_col as f32 * cell_w
            + ((math.cells as f32 * cell_w - math.width) / 2.0).floor();
        let y0 = (baseline - math.baseline)
            .round()
            .min(row_top + cell_h - math.height)
            .max(row_top);
        self.paint_rendered(
            layers,
            &math.rendered,
            (x0, y0),
            (math.width, math.height),
            row_top..row_top + cell_h,
        )
    }

    /// Draws `rendered` as one textured quad at `origin` sized `size`,
    /// clipped vertically to `clip`.
    fn paint_rendered(
        &self,
        layers: &mut TripleLayerQuadAllocator,
        rendered: &Rendered,
        (x0, y0): (f32, f32),
        (width, height): (f32, f32),
        clip: Range<f32>,
    ) -> anyhow::Result<()> {
        let y1 = y0 + height;
        let top = y0.max(clip.start);
        let bottom = y1.min(clip.end);
        if bottom <= top {
            return Ok(());
        }

        let gl_state = self.render_state.as_ref().unwrap();
        let (sprite, _, _) = gl_state.glyph_cache.borrow_mut().cached_image(
            &rendered.image,
            None,
            AllowImage::Yes,
        )?;

        let tex_w = sprite.texture.width() as f32;
        let tex_h = sprite.texture.height() as f32;
        let sprite_w = sprite.coords.size.width as f32;
        let sprite_h = sprite.coords.size.height as f32;
        let v0 = (top - y0) / height;
        let v1 = (bottom - y0) / height;
        let texture = TextureRect::new(
            TextureCoord::new(
                sprite.coords.origin.x as f32 / tex_w,
                (sprite.coords.origin.y as f32 + v0 * sprite_h) / tex_h,
            ),
            TextureSize::new(sprite_w / tex_w, (v1 - v0) * sprite_h / tex_h),
        );

        let left_offset = self.dimensions.pixel_width as f32 / 2.;
        let top_offset = self.dimensions.pixel_height as f32 / 2.;
        let mut quad = layers.allocate(1)?;
        quad.set_position(
            x0 - left_offset,
            top - top_offset,
            x0 + width - left_offset,
            bottom - top_offset,
        );
        quad.set_texture(texture);
        quad.set_fg_color(window::color::LinearRgba::with_components(1., 1., 1., 1.));
        quad.set_hsv(None);
        quad.set_has_color(true);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn lines(text: &str) -> Vec<TextLine> {
        text.lines()
            .enumerate()
            .map(|(i, t)| TextLine {
                row: 100 + i as StableRowIndex,
                rows: 1,
                text: t.to_string(),
            })
            .collect()
    }

    fn find_blocks_text(lines: &[TextLine]) -> Vec<DiagramBlock> {
        find_blocks(lines, true)
    }

    #[test]
    fn finds_claude_block() {
        let blocks = find_blocks_text(&lines(
            "⏺ Here:\n  <wezmux_diagram lang=\"mermaid\">\n  graph LR\n    A --> B\n  </wezmux_diagram>\n  Done.",
        ));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].rows, 101..105);
        assert_eq!(blocks[0].col, 2);
        assert_eq!(&*blocks[0].source, "  graph LR\n    A --> B");
    }

    #[test]
    fn tag_after_bullet() {
        let blocks = find_blocks_text(&lines(
            "⏺ <wezmux_diagram>\n  graph LR\n  A --> B\n  </wezmux_diagram>",
        ));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].col, 2);
        assert_eq!(blocks[0].rows, 100..104);
    }

    #[test]
    fn ignores_prose_mentions_and_other_languages() {
        assert!(find_blocks_text(&lines(
            "use <wezmux_diagram> to draw\ngraph LR\n</wezmux_diagram>"
        ))
        .is_empty());
        assert!(find_blocks_text(&lines(
            "<wezmux_diagram lang=\"dot\">\ndigraph { a -> b }\n</wezmux_diagram>"
        ))
        .is_empty());
        assert!(
            find_blocks_text(&lines("<wezmux_diagramx>\ngraph LR\n</wezmux_diagram>")).is_empty()
        );
    }

    #[test]
    fn unclosed_block_is_left_alone() {
        assert!(find_blocks_text(&lines("<wezmux_diagram>\ngraph LR\n  A --> B")).is_empty());
    }

    #[test]
    fn finds_codex_fallback() {
        let blocks = find_blocks_text(&lines(
            "  I would build this structure:\n\n  This Mermaid diagram exceeds the terminal renderer's size limits.\n  flowchart TD\n      A --> B\n\n  1. Next",
        ));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].rows, 102..105);
        assert_eq!(blocks[0].col, 2);
        assert_eq!(&*blocks[0].source, "  flowchart TD\n      A --> B");
    }

    #[test]
    fn wrapped_rows_count_toward_the_block() {
        let mut l = lines("<wezmux_diagram>\ngraph LR\n</wezmux_diagram>");
        l[1].rows = 2;
        l[2].row += 1;
        assert_eq!(find_blocks(&l, true)[0].rows, 100..104);
    }

    #[test]
    fn finds_display_math_between_diagrams() {
        let text = "<wezmux_diagram>\ngraph LR\n</wezmux_diagram>\n  $$\n  x^2\n  $$";
        let blocks = find_blocks_text(&lines(text));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].kind, RenderKind::DisplayMath);
        assert_eq!(blocks[1].rows, 103..106);
        assert_eq!(&*blocks[1].source, "x^2");
        assert_eq!(find_blocks(&lines(text), false).len(), 1);
    }

    #[test]
    fn renders_in_terminal_colors() {
        let palette = ColorPalette::default();
        let style = DiagramStyle::new(&palette, "Menlo", None, 28.0);
        let rendered =
            render_mermaid("graph LR\n  A[Hook] --> B[Parser]", &style, 2000, 400).unwrap();
        assert!(rendered.width > 100 && rendered.width <= 2000);
        assert!(rendered.height > 10 && rendered.height <= 400);
        assert!(render_mermaid("graph LR\n  A --> B", &style, 40, 10).is_err());
    }
}
