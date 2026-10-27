//! Inline math: LaTeX that an agent prints between `$…$` or `$$…$$` is drawn
//! in place of its source, the same way `diagram.rs` draws Mermaid.
//!
//! Display math (a `$$` line, the formula, and a `$$` line, or `$$…$$` alone
//! on a line) is a block: it takes the rows of its source, and `diagram.rs`
//! scans, caches and paints it alongside the Mermaid blocks. Inline math
//! takes the cells of its source on one row. That row is painted from a copy
//! whose `$…$` cells are collapsed to the formula's width, so the text after
//! the formula closes up to it.
//!
//! Both agent TUIs run their replies through Markdown. Claude Code strips the
//! backslash from `\{`, `\\` and `\,` and turns `_…_` into italics, and Codex
//! rewrites the math as Unicode. Inside a code span or a fenced block the
//! source survives intact, so the prompt asks agents to write math there.
//!
//! RaTeX (a Rust port of KaTeX, with KaTeX's fonts built in) lays out the
//! formula, and resvg rasterizes it in the terminal's foreground color.

use crate::termwindow::diagram::{rasterize, split_indent, DiagramStyle, Rendered, TextLine};
use mux::pane::PaneId;
use ratex_layout::{layout, to_display_list, validate_layout, LayoutOptions};
use ratex_svg::{render_to_svg, SvgOptions};
use ratex_types::color::Color;
use ratex_types::display_item::DisplayList;
use ratex_types::math_style::MathStyle;
use resvg::usvg;
use std::collections::HashMap;
use std::ops::Range;
use termwiz::surface::SequenceNo;
use wezterm_term::{Cell, CellAttributes, Line, StableRowIndex};

const DISPLAY_DELIM: &str = "$$";
/// Longest `$$` block that is looked for, in rows.
pub const MAX_DISPLAY_ROWS: usize = 40;
const MAX_INLINE_LEN: usize = 300;
/// KaTeX sets math at 1.21× the surrounding text so that the x-heights
/// match; here the terminal's font is that text.
const KATEX_SCALE: f32 = 1.21;
/// Display style stacks limits and fractions. Below this scale (a one-row
/// `$$…$$`, say) the flatter inline style is tried as well.
const DISPLAY_MIN_SCALE: f32 = 0.75;
/// Below this the formula would be too small to read, so the source is kept.
const MIN_SCALE: f32 = 0.5;

/// Display math starting at `lines[i]`, as the index of its last line, the
/// column it is drawn from, and its source.
pub fn display_math_at(lines: &[TextLine], i: usize) -> Option<(usize, usize, String)> {
    let (col, body) = split_indent(&lines[i].text);
    let body = body.trim_end();
    if body == DISPLAY_DELIM {
        let window = i + 1..lines.len().min(i + 1 + MAX_DISPLAY_ROWS);
        let end = window
            .clone()
            .find(|&j| lines[j].text.trim() == DISPLAY_DELIM)?;
        let source = lines[i + 1..end]
            .iter()
            .map(|l| l.text.trim())
            .collect::<Vec<_>>()
            .join("\n");
        return (!source.trim().is_empty()).then_some((end, col, source));
    }
    let inner = body
        .strip_prefix(DISPLAY_DELIM)?
        .strip_suffix(DISPLAY_DELIM)?
        .trim();
    (!inner.is_empty() && !inner.contains('$')).then(|| (i, col, inner.to_string()))
}

#[derive(Clone, Debug, PartialEq)]
pub struct InlineSpan {
    /// Cells covered, delimiters included.
    pub cols: Range<usize>,
    pub source: String,
}

/// Whether the text between two `$` reads as math rather than shell
/// variables or prices that happen to pair up.
fn looks_like_math(s: &str) -> bool {
    if s.len() > MAX_INLINE_LEN || !s.chars().any(|c| c.is_alphanumeric()) {
        return false;
    }
    if s.contains([
        '\\', '^', '_', '=', '+', '-', '*', '/', '<', '>', '{', '}', '|',
    ]) {
        return true;
    }
    // A bare term: `n`, `N`, `x'`, `2n`, `f(x)`, but not `HOME` or `PATH:`.
    s.chars().count() <= 12
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "'(),.".contains(c))
        && (s.chars().count() == 1 || s.chars().any(|c| c.is_ascii_lowercase()))
}

/// Finds the `$…$` (and `$$…$$`) spans of one row, given as each char with
/// the column of its cell. Delimiters follow pandoc: the opening `$` is
/// followed by a non-space and the closing `$` is preceded by one and not
/// followed by a digit, so `$5 and $10` stays text.
pub fn find_inline_math(chars: &[(usize, char)]) -> Vec<InlineSpan> {
    let ch = |i: usize| chars.get(i).map(|&(_, c)| c);
    let is_dollar = |i: usize| ch(i) == Some('$') && (i == 0 || ch(i - 1) != Some('\\'));

    let mut spans = vec![];
    let mut i = 0;
    while i < chars.len() {
        if !is_dollar(i) {
            i += 1;
            continue;
        }
        let double = is_dollar(i + 1);
        let delim = if double { 2 } else { 1 };
        let start = i + delim;
        let Some(close) = (start..chars.len()).find(|&j| is_dollar(j)) else {
            break;
        };
        let after = close + delim;
        let content: String = chars[start..close].iter().map(|&(_, c)| c).collect();

        let delimited = if double {
            is_dollar(close + 1)
        } else {
            let before_open = if i == 0 { None } else { ch(i - 1) };
            !before_open.is_some_and(|c| c.is_alphanumeric())
                && ch(start).is_some_and(|c| !c.is_whitespace())
                && ch(close - 1).is_some_and(|c| !c.is_whitespace())
                && !ch(after).is_some_and(|c| c.is_alphanumeric())
        };
        if delimited && looks_like_math(content.trim()) {
            spans.push(InlineSpan {
                cols: chars[i].0..chars[after - 1].0 + 1,
                source: content.trim().to_string(),
            });
            i = after;
        } else {
            // The closing `$` may open the next span.
            i = close.max(i + 1);
        }
    }
    spans
}

fn lay_out(
    ast: &[ratex_parser::ParseNode],
    style: MathStyle,
    color: Color,
) -> anyhow::Result<DisplayList> {
    let options = LayoutOptions::default().with_style(style).with_color(color);
    let root = layout(ast, &options);
    validate_layout(&root).map_err(anyhow::Error::msg)?;
    Ok(to_display_list(&root))
}

/// Lays out `source` and rasterizes it to fit `box_w` x `box_h` device
/// pixels, never above the terminal's text size. `display` picks display
/// style (`$$`) over inline style (`$`).
pub fn render_math(
    source: &str,
    display: bool,
    style: &DiagramStyle,
    box_w: u32,
    box_h: u32,
) -> anyhow::Result<Rendered> {
    let ast = ratex_parser::parse(source).map_err(|e| anyhow::anyhow!("{e}"))?;
    let (r, g, b) = style.foreground;
    let color = Color::rgb(r as f32 / 255., g as f32 / 255., b as f32 / 255.);
    let em = style.font_px as f32 * KATEX_SCALE;
    let fit = |list: &DisplayList| {
        let width = list.width as f32 * em;
        let height = (list.height + list.depth) as f32 * em;
        (box_w as f32 / width).min(box_h as f32 / height).min(1.0)
    };

    let math_style = if display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
    let mut list = lay_out(&ast, math_style, color)?;
    let mut scale = fit(&list);
    if display && scale < DISPLAY_MIN_SCALE {
        let flat = lay_out(&ast, MathStyle::Text, color)?;
        if fit(&flat) > scale {
            scale = fit(&flat);
            list = flat;
        }
    }
    anyhow::ensure!(
        scale >= MIN_SCALE,
        "math needs {:.0}x{:.0}px, has {box_w}x{box_h}",
        list.width as f32 * em,
        (list.height + list.depth) as f32 * em,
    );

    // Laid out at the final size, so glyphs are rasterized crisp rather
    // than scaled.
    let svg = render_to_svg(
        &list,
        &SvgOptions {
            font_size: (em * scale) as f64,
            padding: 0.0,
            embed_glyphs: true,
            ..SvgOptions::default()
        },
    );
    // Glyphs are embedded as paths, so no font database is needed.
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())?;
    let mut rendered = rasterize(&tree, 1.0)?;
    rendered.baseline = list.height as f32 * em * scale;
    Ok(rendered)
}

/// Inline math ready to draw over a row.
pub struct PlacedInline {
    pub row: StableRowIndex,
    /// Cells of the source, delimiters included.
    pub cols: Range<usize>,
    /// Cells the formula takes once the row is collapsed.
    pub cells: usize,
    /// Column the formula is drawn from once the row is collapsed.
    pub draw_col: usize,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub rendered: Rendered,
}

/// Where column `col` of a row lands once the row's spans (sorted by
/// column) are collapsed.
pub fn collapsed_col(spans: &[&PlacedInline], col: usize) -> usize {
    let mut shift = 0;
    for span in spans {
        if col >= span.cols.end {
            shift += span.cols.len() - span.cells;
        } else if col > span.cols.start {
            return span.cols.start - shift + (col - span.cols.start).min(span.cells);
        }
    }
    col - shift
}

/// A copy of `line` in which each span's cells become `cells` blank cells,
/// so the text after a formula closes up to it.
pub fn collapse_line(line: &Line, spans: &[&PlacedInline]) -> Line {
    let mut source = line.clone();
    let cells = source.cells_mut();
    let mut out: Vec<Cell> = Vec::with_capacity(cells.len());
    let mut col = 0;
    for span in spans {
        let start = span.cols.start.min(cells.len());
        out.extend_from_slice(&cells[col..start]);
        // The blanks keep the background of the text around the formula.
        let mut attrs = CellAttributes::blank();
        if let Some(neighbor) = cells.get(start.saturating_sub(1)) {
            attrs.set_background(neighbor.attrs().background());
        }
        out.extend(std::iter::repeat_n(Cell::new(' ', attrs), span.cells));
        col = span.cols.end.min(cells.len());
    }
    out.extend_from_slice(&cells[col..]);
    Line::from_cells(out, line.current_seqno())
}

struct CollapsedLine {
    seqno: SequenceNo,
    spans: Vec<(Range<usize>, usize)>,
    line: Line,
}

/// Collapsed copies of rows with inline math, kept across frames so that
/// the line's cached shape hash survives with it.
#[derive(Default)]
pub struct CollapsedLines {
    lines: HashMap<(PaneId, StableRowIndex), CollapsedLine>,
}

const MAX_COLLAPSED_LINES: usize = 1024;

impl CollapsedLines {
    /// Takes the collapsed copy of `line` out of the cache, building it if
    /// the row or its spans changed. Hand it back with [`Self::put`].
    pub fn take(
        &mut self,
        pane_id: PaneId,
        row: StableRowIndex,
        line: &Line,
        spans: &[&PlacedInline],
    ) -> Line {
        let key: Vec<_> = spans.iter().map(|s| (s.cols.clone(), s.cells)).collect();
        match self.lines.remove(&(pane_id, row)) {
            Some(cached) if cached.seqno == line.current_seqno() && cached.spans == key => {
                cached.line
            }
            _ => collapse_line(line, spans),
        }
    }

    pub fn put(
        &mut self,
        pane_id: PaneId,
        row: StableRowIndex,
        line: Line,
        spans: &[&PlacedInline],
    ) {
        if self.lines.len() >= MAX_COLLAPSED_LINES {
            self.lines.clear();
        }
        let spans = spans.iter().map(|s| (s.cols.clone(), s.cells)).collect();
        self.lines.insert(
            (pane_id, row),
            CollapsedLine {
                seqno: line.current_seqno(),
                spans,
                line,
            },
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use wezterm_term::color::ColorPalette;

    fn spans(text: &str) -> Vec<(Range<usize>, String)> {
        let chars: Vec<_> = text.chars().enumerate().collect();
        find_inline_math(&chars)
            .into_iter()
            .map(|s| (s.cols, s.source))
            .collect()
    }

    fn lines(text: &str) -> Vec<TextLine> {
        text.lines()
            .enumerate()
            .map(|(i, t)| TextLine {
                row: i as StableRowIndex,
                rows: 1,
                text: t.to_string(),
            })
            .collect()
    }

    #[test]
    fn finds_inline_math() {
        assert_eq!(
            spans("energy $E = mc^2$ where $n$ is odd"),
            vec![(7..17, "E = mc^2".into()), (24..27, "n".into())]
        );
        assert_eq!(
            spans(r"so $\alpha_{i}$."),
            vec![(3..15, r"\alpha_{i}".into())]
        );
        assert_eq!(spans("then $$x^2$$ holds"), vec![(5..12, "x^2".into())]);
    }

    #[test]
    fn leaves_shell_and_prices_alone() {
        assert!(spans("costs $5 and $10 each").is_empty());
        assert!(spans("costs $5,$10 each").is_empty());
        assert!(spans(r#"echo "$HOME" "$PATH""#).is_empty());
        assert!(spans("cp $HOME/$USER/x .").is_empty());
        assert!(spans("echo $PATH:$HOME").is_empty());
        assert!(spans("awk '{print $1 + $2}'").is_empty());
        assert!(spans("US$5 or US$6").is_empty());
        assert!(spans(r"an escaped \$x\$ pair").is_empty());
        assert!(spans("echo $HOME$").is_empty());
    }

    #[test]
    fn closing_dollar_can_open_the_next_span() {
        assert_eq!(spans("pay $5 for $x^2$"), vec![(11..16, "x^2".into())]);
    }

    #[test]
    fn finds_display_blocks() {
        let l = lines("Then:\n  $$\n  \\frac{a}{b}\n  $$\nDone.");
        assert_eq!(display_math_at(&l, 0), None);
        assert_eq!(display_math_at(&l, 1), Some((3, 2, r"\frac{a}{b}".into())));
        let l = lines("⏺ $$\\sum_i i$$");
        assert_eq!(display_math_at(&l, 0), Some((0, 2, r"\sum_i i".into())));
        assert_eq!(display_math_at(&lines("$$\n$$"), 0), None);
        assert_eq!(display_math_at(&lines("$$ $a$ $$"), 0), None);
        assert_eq!(display_math_at(&lines("$$\nx\n"), 0), None);
    }

    fn placed(cols: Range<usize>, cells: usize) -> PlacedInline {
        PlacedInline {
            row: 0,
            cols,
            cells,
            draw_col: 0,
            width: 0.,
            height: 0.,
            baseline: 0.,
            rendered: Rendered::blank(),
        }
    }

    #[test]
    fn collapses_columns() {
        let a = placed(2..10, 3);
        let b = placed(12..15, 1);
        let spans = [&a, &b];
        assert_eq!(collapsed_col(&spans, 0), 0);
        assert_eq!(collapsed_col(&spans, 2), 2);
        assert_eq!(collapsed_col(&spans, 4), 4);
        assert_eq!(collapsed_col(&spans, 9), 5);
        assert_eq!(collapsed_col(&spans, 10), 5);
        assert_eq!(collapsed_col(&spans, 12), 7);
        assert_eq!(collapsed_col(&spans, 15), 8);
        assert_eq!(collapsed_col(&spans, 20), 13);
    }

    #[test]
    fn collapses_line() {
        let line = Line::from_text("a $x^2$ b", &CellAttributes::default(), 0, None);
        let span = placed(2..7, 2);
        let collapsed = collapse_line(&line, &[&span]);
        assert_eq!(collapsed.as_str(), "a    b");
    }

    #[test]
    fn renders_in_both_styles() {
        let palette = ColorPalette::default();
        let style = DiagramStyle::new(&palette, "Menlo", None, 28.0);
        let display = render_math(r"\sum_{i=1}^n i", true, &style, 2000, 200).unwrap();
        let inline = render_math(r"\sum_{i=1}^n i", false, &style, 2000, 200).unwrap();
        assert!(display.height > inline.height);
        assert!(inline.baseline > 0. && inline.baseline < inline.height as f32);
        // One row is too short for display style, so inline style is used.
        let row = render_math(r"\sum_{i=1}^n i", true, &style, 2000, 60).unwrap();
        assert!(row.height <= 60);
        assert!(render_math(r"\frac{a", false, &style, 2000, 60).is_err());
        assert!(render_math(
            r"\begin{pmatrix}a\\b\\c\\d\end{pmatrix}",
            false,
            &style,
            2000,
            34
        )
        .is_err());
    }
}
