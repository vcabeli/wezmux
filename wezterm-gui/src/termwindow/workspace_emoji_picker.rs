//! Modal that presents every emoji in a clickable grid for use as a workspace
//! decorator. Replaces the previous curated 12-emoji native submenu.

use crate::termwindow::box_model::*;
use crate::termwindow::modal::Modal;
use crate::termwindow::{DimensionContext, UIItemType};
use crate::utilsprites::RenderMetrics;
use crate::TermWindow;
use config::keyassignment::KeyAssignment;
use config::Dimension;
use emojis::Emoji;
use std::cell::{Ref, RefCell};
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};
use window::color::LinearRgba;

/// Number of emoji cells per row.
const COLUMNS: usize = 8;
/// How many rows of emoji we show before requiring scroll.
const VISIBLE_ROWS: usize = 10;
/// Per-cell footprint in cells of the picker font.
const CELL_WIDTH_CELLS: f32 = 3.0;

pub struct WorkspaceEmojiPicker {
    workspace: String,
    emojis: Vec<&'static Emoji>,
    filter: RefCell<String>,
    /// Indices into `emojis` matching the current filter.
    matches: RefCell<Vec<usize>>,
    /// Row offset (in grid rows) for vertical scroll.
    top_row: RefCell<usize>,
    element: RefCell<Option<Vec<ComputedElement>>>,
}

impl WorkspaceEmojiPicker {
    pub fn new(workspace: String) -> Self {
        let emojis: Vec<&'static Emoji> = emojis::iter().collect();
        let matches = (0..emojis.len()).collect();
        Self {
            workspace,
            emojis,
            filter: RefCell::new(String::new()),
            matches: RefCell::new(matches),
            top_row: RefCell::new(0),
            element: RefCell::new(None),
        }
    }

    /// Workspace this picker is editing.
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// Adjust the visible row offset, clamped to the valid range.
    pub fn scroll_by(&self, rows: isize) {
        let total_rows = self.total_rows();
        let max_top = total_rows.saturating_sub(VISIBLE_ROWS);
        let mut top = self.top_row.borrow_mut();
        let next = (*top as isize + rows).max(0) as usize;
        *top = next.min(max_top);
    }

    fn total_rows(&self) -> usize {
        let len = self.matches.borrow().len();
        len.div_ceil(COLUMNS).max(1)
    }

    fn rebuild_matches(&self) {
        let filter = self.filter.borrow();
        let needle = filter.trim().to_lowercase();
        let next: Vec<usize> = self
            .emojis
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if needle.is_empty() {
                    return true;
                }
                if e.name().to_lowercase().contains(&needle) {
                    return true;
                }
                e.shortcodes().any(|s| s.to_lowercase().contains(&needle))
            })
            .map(|(idx, _)| idx)
            .collect();
        *self.matches.borrow_mut() = next;
        *self.top_row.borrow_mut() = 0;
    }

    fn build_root(&self, term_window: &mut TermWindow) -> anyhow::Result<Vec<ComputedElement>> {
        let font = term_window.fonts.char_select_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());

        let fg = term_window.config.char_select_fg_color.to_linear();
        let bg = term_window.config.char_select_bg_color.to_linear();
        let muted = LinearRgba(fg.0, fg.1, fg.2, fg.3 * 0.55);
        let accent = LinearRgba::with_srgba(0, 145, 255, 255);
        let cell_hover = LinearRgba(fg.0, fg.1, fg.2, 0.18);

        let matches = self.matches.borrow();
        let top_row = *self.top_row.borrow();
        let total_rows = self.total_rows();
        let max_top = total_rows.saturating_sub(VISIBLE_ROWS);
        let effective_top = top_row.min(max_top);

        // Header: title on the left, Reset / Cancel on the right.
        let title_label = format!("Workspace Emoji — {}", self.workspace);
        let title = Element::new(&font, ElementContent::Text(title_label)).colors(ElementColors {
            border: BorderColor::default(),
            bg: InheritableColor::Inherited,
            text: fg.into(),
        });

        let action_button = |label: &str, item_type: UIItemType, color: LinearRgba| -> Element {
            Element::new(&font, ElementContent::Text(label.to_string()))
                .item_type(item_type)
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: InheritableColor::Inherited,
                    text: color.into(),
                })
                .hover_colors(Some(ElementColors {
                    border: BorderColor::default(),
                    bg: cell_hover.into(),
                    text: color.into(),
                }))
                .padding(BoxDimension {
                    left: Dimension::Cells(0.4),
                    right: Dimension::Cells(0.4),
                    top: Dimension::Cells(0.1),
                    bottom: Dimension::Cells(0.1),
                })
                .margin(BoxDimension {
                    left: Dimension::Cells(0.5),
                    right: Dimension::Pixels(0.),
                    top: Dimension::Pixels(0.),
                    bottom: Dimension::Pixels(0.),
                })
        };

        let header_children = vec![
            title,
            action_button("Reset", UIItemType::WorkspaceEmojiReset, accent).float(Float::Right),
            action_button("Close", UIItemType::WorkspaceEmojiCancel, muted).float(Float::Right),
        ];

        let header = Element::new(&font, ElementContent::Children(header_children))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Cells(0.5),
                right: Dimension::Cells(0.5),
                top: Dimension::Cells(0.25),
                bottom: Dimension::Cells(0.5),
            });

        // Filter / status line: shows the user-typed substring (or hint).
        let filter = self.filter.borrow();
        let filter_text = if filter.is_empty() {
            "Type to filter… · ↑/↓ scroll · Esc cancel".to_string()
        } else {
            format!("Filter: {}_ ({} matches)", filter.as_str(), matches.len())
        };
        let filter_line = Element::new(&font, ElementContent::Text(filter_text))
            .display(DisplayType::Block)
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: InheritableColor::Inherited,
                text: muted.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Cells(0.5),
                right: Dimension::Cells(0.5),
                top: Dimension::Pixels(0.),
                bottom: Dimension::Cells(0.5),
            });

        // Grid body: rows of emoji cells.
        let mut grid_rows: Vec<Element> = vec![];
        if matches.is_empty() {
            grid_rows.push(
                Element::new(
                    &font,
                    ElementContent::Text("No emoji matches that filter.".to_string()),
                )
                .display(DisplayType::Block)
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: InheritableColor::Inherited,
                    text: muted.into(),
                })
                .padding(BoxDimension {
                    left: Dimension::Cells(0.5),
                    right: Dimension::Cells(0.5),
                    top: Dimension::Cells(0.5),
                    bottom: Dimension::Cells(0.5),
                }),
            );
        } else {
            let start = effective_top * COLUMNS;
            let end = (start + VISIBLE_ROWS * COLUMNS).min(matches.len());
            for row_slice in matches[start..end].chunks(COLUMNS) {
                let mut row_cells: Vec<Element> = vec![];
                for &emoji_idx in row_slice {
                    let emoji = self.emojis[emoji_idx];
                    let glyph = emoji.as_str().to_string();
                    let cell = Element::new(&font, ElementContent::Text(glyph.clone()))
                        .item_type(UIItemType::WorkspaceEmojiCell(glyph))
                        .colors(ElementColors {
                            border: BorderColor::default(),
                            bg: InheritableColor::Inherited,
                            text: fg.into(),
                        })
                        .hover_colors(Some(ElementColors {
                            border: BorderColor::default(),
                            bg: cell_hover.into(),
                            text: fg.into(),
                        }))
                        .min_width(Some(Dimension::Cells(CELL_WIDTH_CELLS)))
                        .padding(BoxDimension {
                            left: Dimension::Cells(0.3),
                            right: Dimension::Cells(0.3),
                            top: Dimension::Cells(0.15),
                            bottom: Dimension::Cells(0.15),
                        });
                    row_cells.push(cell);
                }
                grid_rows.push(
                    Element::new(&font, ElementContent::Children(row_cells))
                        .display(DisplayType::Block),
                );
            }

            // Trailing scroll indicator (visible when more rows exist below).
            let hidden_below = total_rows.saturating_sub(effective_top + VISIBLE_ROWS);
            if hidden_below > 0 {
                grid_rows.push(
                    Element::new(
                        &font,
                        ElementContent::Text(format!("↓ {hidden_below} more rows")),
                    )
                    .display(DisplayType::Block)
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg: InheritableColor::Inherited,
                        text: muted.into(),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.5),
                        top: Dimension::Cells(0.2),
                        bottom: Dimension::Cells(0.2),
                    }),
                );
            }
        }

        let body = Element::new(&font, ElementContent::Children(grid_rows))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Cells(0.25),
                right: Dimension::Cells(0.25),
                top: Dimension::Pixels(0.),
                bottom: Dimension::Cells(0.25),
            });

        let root = Element::new(
            &font,
            ElementContent::Children(vec![header, filter_line, body]),
        )
        .display(DisplayType::Block)
        .item_type(UIItemType::WorkspaceEmojiBackground)
        .colors(ElementColors {
            border: BorderColor::new(bg),
            bg: bg.into(),
            text: fg.into(),
        })
        .border(BoxDimension::new(Dimension::Pixels(1.)))
        .padding(BoxDimension {
            left: Dimension::Cells(0.5),
            right: Dimension::Cells(0.5),
            top: Dimension::Cells(0.5),
            bottom: Dimension::Cells(0.5),
        })
        .margin(BoxDimension {
            left: Dimension::Cells(1.0),
            right: Dimension::Cells(1.0),
            top: Dimension::Cells(1.0),
            bottom: Dimension::Cells(1.0),
        });

        let dimensions = term_window.dimensions;
        let size = term_window.terminal_size;

        let top_bar_height = if term_window.show_tab_bar && !term_window.config.tab_bar_at_bottom {
            term_window.tab_bar_pixel_height().unwrap_or(0.)
        } else {
            0.
        };
        let (padding_left, padding_top) = term_window.padding_left_top();
        let border = term_window.get_os_border();
        let sidebar_w = term_window.sidebar_pixel_width();
        let top_pixel_y = top_bar_height + padding_top + border.top.get() as f32;
        let left_x = padding_left + border.left.get() as f32 + sidebar_w;
        let pixel_w = size.cols as f32 * term_window.render_metrics.cell_size.width as f32
            - sidebar_w
            - border.left.get() as f32
            - border.right.get() as f32;
        let pixel_h = size.rows as f32 * term_window.render_metrics.cell_size.height as f32;

        let computed = term_window.compute_element(
            &LayoutContext {
                height: DimensionContext {
                    dpi: dimensions.dpi as f32,
                    pixel_max: dimensions.pixel_height as f32,
                    pixel_cell: metrics.cell_size.height as f32,
                },
                width: DimensionContext {
                    dpi: dimensions.dpi as f32,
                    pixel_max: dimensions.pixel_width as f32,
                    pixel_cell: metrics.cell_size.width as f32,
                },
                bounds: euclid::rect(left_x, top_pixel_y, pixel_w.max(1.0), pixel_h),
                metrics: &metrics,
                gl_state: term_window.render_state.as_ref().unwrap(),
                zindex: 100,
            },
            &root,
        )?;

        Ok(vec![computed])
    }
}

impl Modal for WorkspaceEmojiPicker {
    fn perform_assignment(
        &self,
        _assignment: &KeyAssignment,
        _term_window: &mut TermWindow,
    ) -> bool {
        false
    }

    fn mouse_event(&self, _event: MouseEvent, _term_window: &mut TermWindow) -> anyhow::Result<()> {
        Ok(())
    }

    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool> {
        match (key, mods) {
            (KeyCode::Escape, KeyModifiers::NONE) | (KeyCode::Char('g'), KeyModifiers::CTRL) => {
                term_window.cancel_modal();
                return Ok(true);
            }
            (KeyCode::UpArrow, KeyModifiers::NONE) => self.scroll_by(-1),
            (KeyCode::DownArrow, KeyModifiers::NONE) => self.scroll_by(1),
            (KeyCode::PageUp, KeyModifiers::NONE) => self.scroll_by(-(VISIBLE_ROWS as isize)),
            (KeyCode::PageDown, KeyModifiers::NONE) => self.scroll_by(VISIBLE_ROWS as isize),
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                self.filter.borrow_mut().push(c);
                self.rebuild_matches();
            }
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.filter.borrow_mut().pop();
                self.rebuild_matches();
            }
            (KeyCode::Char('u'), KeyModifiers::CTRL) => {
                self.filter.borrow_mut().clear();
                self.rebuild_matches();
            }
            _ => return Ok(false),
        }
        term_window.invalidate_modal();
        Ok(true)
    }

    fn computed_element(
        &self,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<'_, [ComputedElement]>> {
        if self.element.borrow().is_none() {
            let elements = self.build_root(term_window)?;
            self.element.borrow_mut().replace(elements);
        }
        Ok(Ref::map(self.element.borrow(), |v| {
            v.as_ref().unwrap().as_slice()
        }))
    }

    fn reconfigure(&self, _term_window: &mut TermWindow) {
        self.element.borrow_mut().take();
    }
}
