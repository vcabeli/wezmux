use crate::quad::TripleLayerQuadAllocator;
use crate::termwindow::box_model::*;
use crate::termwindow::sidebar::{
    AgentStatus, WorkspaceEntry, WorkspacePullRequest, WorkspacePullRequestStatus,
};
use crate::termwindow::{UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use config::{Dimension, DimensionContext};
use std::env;
use std::path::Path;
use std::rc::Rc;
use wezterm_font::LoadedFont;
use window::color::LinearRgba;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarLineStyle {
    Preview,
    Meta,
    Secondary,
    PullRequest(WorkspacePullRequestStatus),
    StatusIndicator(AgentStatus),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SidebarLine {
    text: String,
    style: SidebarLineStyle,
}

// Sidebar bg: lighter gray
fn sidebar_bg() -> LinearRgba {
    LinearRgba::with_srgba(58, 58, 65, 255)
}

// Inactive cards: slightly darker than sidebar
fn sidebar_card_bg() -> LinearRgba {
    LinearRgba::with_srgba(48, 48, 54, 255)
}

// Hover: brighter
fn sidebar_card_hover() -> LinearRgba {
    LinearRgba::with_srgba(68, 68, 75, 255)
}

// Active card: cmux accent blue #0091FF
fn sidebar_card_active() -> LinearRgba {
    LinearRgba::with_srgba(0, 145, 255, 255)
}

// Primary text (inactive)
fn sidebar_text() -> LinearRgba {
    LinearRgba::with_srgba(255, 255, 255, 230)
}

// Secondary text (inactive) -- maps to .secondary
fn sidebar_muted() -> LinearRgba {
    LinearRgba::with_srgba(255, 255, 255, 140)
}

// Active card: white text at various opacities
fn sidebar_active_text() -> LinearRgba {
    LinearRgba::with_srgba(255, 255, 255, 255)
}

fn sidebar_active_muted() -> LinearRgba {
    LinearRgba::with_srgba(255, 255, 255, 191)
}

fn sidebar_active_secondary() -> LinearRgba {
    LinearRgba::with_srgba(255, 255, 255, 148)
}

fn sidebar_separator() -> LinearRgba {
    LinearRgba::with_srgba(255, 255, 255, 25)
}

// cmux accent blue #0091FF
fn sidebar_accent() -> LinearRgba {
    LinearRgba::with_srgba(0, 145, 255, 255)
}

fn sidebar_pull_request_open() -> LinearRgba {
    LinearRgba::with_srgba(184, 96, 255, 255)
}

fn sidebar_pull_request_merged() -> LinearRgba {
    LinearRgba::with_srgba(76, 197, 124, 255)
}

fn sidebar_pull_request_closed() -> LinearRgba {
    LinearRgba::with_srgba(215, 106, 106, 255)
}

fn sidebar_entry_body_lines(entry: &WorkspaceEntry, cols: usize) -> Vec<SidebarLine> {
    let mut lines = vec![];

    // Notification preview, agent status, or terminal output preview
    if let Some(ref agent) = entry.agent {
        // Pick the best preview text: terminal buffer > notification > nothing.
        // When idle, prefer terminal output (shows actual response) over generic
        // "Claude finished". Fall through if terminal preview is unavailable.
        let idle_or_unknown =
            agent.status == AgentStatus::Idle || agent.status == AgentStatus::Unknown;
        let preview_text = if idle_or_unknown {
            entry
                .terminal_preview
                .as_deref()
                .or(agent.status_message.as_deref())
        } else {
            agent
                .status_message
                .as_deref()
                .or(entry.terminal_preview.as_deref())
        };
        if let Some(text) = preview_text {
            for line in wrap_text_to_cells(text, cols, 4) {
                lines.push(SidebarLine {
                    text: line,
                    style: SidebarLineStyle::Preview,
                });
            }
        }
        let label = agent_status_label(agent.status);
        if !label.is_empty() {
            lines.push(SidebarLine {
                text: format!("{} {}", agent_status_symbol(agent.status), label),
                style: SidebarLineStyle::StatusIndicator(agent.status),
            });
        }
    } else if let Some(preview) = sidebar_notification_preview(entry) {
        for line in wrap_text_to_cells(&preview, cols, 4) {
            lines.push(SidebarLine {
                text: line,
                style: SidebarLineStyle::Preview,
            });
        }
    } else if let Some(ref preview) = entry.terminal_preview {
        // Fallback: show last terminal output when no agent or notification
        for line in wrap_text_to_cells(preview, cols, 4) {
            lines.push(SidebarLine {
                text: line,
                style: SidebarLineStyle::Preview,
            });
        }
    }

    // Git branch line (always show separately when available)
    if let Some(branch) = entry.git_branch.as_ref() {
        let branch_text = if entry.git_dirty {
            format!("{branch}*")
        } else {
            branch.clone()
        };
        // Combine with path using separator
        if let Some(path) = sidebar_entry_path(entry) {
            lines.push(SidebarLine {
                text: format!("{branch_text} \u{2022} {path}"),
                style: SidebarLineStyle::Meta,
            });
        } else {
            lines.push(SidebarLine {
                text: branch_text,
                style: SidebarLineStyle::Meta,
            });
        }
    } else if let Some(path) = sidebar_entry_path(entry) {
        // No git branch — just show path
        lines.push(SidebarLine {
            text: path,
            style: SidebarLineStyle::Meta,
        });
    }

    // PR info
    if let Some(pull_request) = sidebar_entry_pull_request(entry) {
        lines.push(pull_request);
    }

    // Listening ports
    if let Some(secondary) = sidebar_entry_secondary(entry) {
        lines.push(SidebarLine {
            text: secondary,
            style: SidebarLineStyle::Secondary,
        });
    }

    lines.truncate(10);
    lines
}

fn agent_status_symbol(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::NeedsInput => "\u{25B2}",
        AgentStatus::Idle => "\u{25CF}",
        AgentStatus::Working => "\u{25B6}",
        AgentStatus::Unknown => "\u{25CB}",
    }
}

fn agent_status_label(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::NeedsInput => "Needs input",
        AgentStatus::Idle => "Idle",
        AgentStatus::Working => "Working",
        AgentStatus::Unknown => "",
    }
}

fn sidebar_status_color(status: AgentStatus, is_active: bool) -> LinearRgba {
    if is_active {
        // On blue active card, use white at varying opacity
        return match status {
            AgentStatus::NeedsInput => LinearRgba::with_srgba(255, 255, 255, 204),
            AgentStatus::Idle => LinearRgba::with_srgba(255, 255, 255, 204),
            AgentStatus::Working => LinearRgba::with_srgba(255, 255, 255, 204),
            AgentStatus::Unknown => sidebar_active_muted(),
        };
    }
    match status {
        AgentStatus::NeedsInput => LinearRgba::with_srgba(0, 145, 255, 255),
        AgentStatus::Idle => LinearRgba::with_srgba(76, 197, 124, 255),
        AgentStatus::Working => LinearRgba::with_srgba(253, 151, 31, 255),
        AgentStatus::Unknown => sidebar_muted(),
    }
}

fn sidebar_entry_path(entry: &WorkspaceEntry) -> Option<String> {
    compact_path(entry.cwd_path.as_deref()).or_else(|| entry.cwd.clone())
}

fn sidebar_entry_secondary(entry: &WorkspaceEntry) -> Option<String> {
    format_listening_ports(&entry.listening_ports)
}

fn sidebar_notification_preview(entry: &WorkspaceEntry) -> Option<String> {
    entry.latest_notification.as_ref().and_then(|notification| {
        let collapsed = notification
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if collapsed.is_empty() {
            None
        } else {
            Some(collapsed)
        }
    })
}

fn sidebar_entry_pull_request(entry: &WorkspaceEntry) -> Option<SidebarLine> {
    entry.pull_request.as_ref().map(|pull_request| SidebarLine {
        text: sidebar_pull_request_text(pull_request),
        style: SidebarLineStyle::PullRequest(pull_request.status),
    })
}

fn sidebar_pull_request_text(pull_request: &WorkspacePullRequest) -> String {
    format!(
        "{} PR #{} {}",
        sidebar_pull_request_symbol(pull_request.status),
        pull_request.number,
        sidebar_pull_request_label(pull_request.status)
    )
}

fn sidebar_pull_request_symbol(status: WorkspacePullRequestStatus) -> &'static str {
    match status {
        WorkspacePullRequestStatus::Open => "◦",
        WorkspacePullRequestStatus::Merged => "✓",
        WorkspacePullRequestStatus::Closed => "✕",
    }
}

fn sidebar_pull_request_label(status: WorkspacePullRequestStatus) -> &'static str {
    match status {
        WorkspacePullRequestStatus::Open => "open",
        WorkspacePullRequestStatus::Merged => "merged",
        WorkspacePullRequestStatus::Closed => "closed",
    }
}

fn compact_path(path: Option<&Path>) -> Option<String> {
    let path = path?;
    let home = env::var_os("HOME").map(std::path::PathBuf::from);

    let display = if let Some(home) = home.as_ref() {
        if let Ok(stripped) = path.strip_prefix(home) {
            format!("~/{}", stripped.display())
        } else {
            path.display().to_string()
        }
    } else {
        path.display().to_string()
    };

    Some(display)
}

fn format_listening_ports(ports: &[u16]) -> Option<String> {
    if ports.is_empty() {
        return None;
    }

    let visible = ports
        .iter()
        .take(3)
        .map(|p| format!(":{p}"))
        .collect::<Vec<_>>()
        .join(", ");
    if ports.len() > 3 {
        Some(format!("{visible} +{}", ports.len() - 3))
    } else {
        Some(visible)
    }
}

/// Helper to create a text Element with specific foreground and background colors.
/// Helper to create a text Element with specific foreground and inherited background.
fn text_element_fg(font: &Rc<LoadedFont>, text: &str, fg: LinearRgba) -> Element {
    Element::new(font, ElementContent::Text(text.to_string())).colors(ElementColors {
        border: BorderColor::default(),
        bg: InheritableColor::Inherited,
        text: fg.into(),
    })
}

/// Build an Element tree for one workspace card.
fn build_card_element(
    font: &Rc<LoadedFont>,
    body_font: &Rc<LoadedFont>,
    mono_font: &Rc<LoadedFont>,
    entry: &WorkspaceEntry,
    text_cols: usize,
    card_width: f32,
) -> Element {
    let is_active = entry.is_active;
    let card_bg = if is_active {
        sidebar_card_active()
    } else {
        sidebar_card_bg()
    };
    let title_color = if is_active {
        sidebar_active_text()
    } else {
        sidebar_text()
    };

    let mut card_children: Vec<Element> = vec![];

    // Title line (with optional unread badge prefix and agent icon)
    let title_text = if let Some(ref agent) = entry.agent {
        format!("\u{2731} {}", agent.display_name)
    } else {
        entry.title.clone()
    };

    let mut title_parts: Vec<Element> = vec![];
    if entry.unread_count > 0 {
        let count_text = if entry.unread_count > 9 {
            "9+".to_string()
        } else {
            entry.unread_count.to_string()
        };
        let badge_bg = if is_active {
            LinearRgba::with_srgba(255, 255, 255, 64)
        } else {
            sidebar_accent()
        };
        title_parts.push(
            Element::new(font, ElementContent::Text(format!(" {} ", count_text)))
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: badge_bg.into(),
                    text: LinearRgba::with_srgba(255, 255, 255, 255).into(),
                })
                .margin(BoxDimension {
                    left: Dimension::Pixels(0.),
                    right: Dimension::Pixels(6.),
                    top: Dimension::Pixels(0.),
                    bottom: Dimension::Pixels(0.),
                }),
        );
    }
    title_parts.push(text_element_fg(
        font,
        &title_text,
        title_color,
    ));

    // Close button (×) — shown on hover
    if entry.is_hovered || is_active {
        let close_color = if is_active {
            LinearRgba::with_srgba(255, 255, 255, 140)
        } else {
            LinearRgba::with_srgba(255, 255, 255, 100)
        };
        title_parts.push(
            Element::new(font, ElementContent::Text("\u{00D7}".to_string()))
                .item_type(UIItemType::SidebarCloseWorkspace(entry.name.clone()))
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: InheritableColor::Inherited,
                    text: close_color.into(),
                })
                .margin(BoxDimension {
                    left: Dimension::Pixels(4.),
                    right: Dimension::Pixels(0.),
                    top: Dimension::Pixels(0.),
                    bottom: Dimension::Pixels(0.),
                }),
        );
    }

    card_children.push(
        Element::new(font, ElementContent::Children(title_parts))
            .display(DisplayType::Block)
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: InheritableColor::Inherited,
                text: InheritableColor::Inherited,
            }),
    );

    // Body lines — use body_font for preview/status, mono_font for meta/path
    let body_lines = sidebar_entry_body_lines(entry, text_cols);
    for line in &body_lines {
        let (line_font, fg) = match line.style {
            SidebarLineStyle::Preview => {
                let fg = if is_active {
                    sidebar_active_muted()
                } else {
                    sidebar_muted()
                };
                (body_font, fg)
            }
            SidebarLineStyle::Meta | SidebarLineStyle::Secondary => {
                let fg = if is_active {
                    sidebar_active_secondary()
                } else {
                    sidebar_muted()
                };
                (mono_font, fg)
            }
            SidebarLineStyle::PullRequest(status) => {
                (mono_font, sidebar_pull_request_color(status, is_active))
            }
            SidebarLineStyle::StatusIndicator(status) => {
                (body_font, sidebar_status_color(status, is_active))
            }
        };
        card_children.push(
            text_element_fg(line_font, &line.text, fg)
                .display(DisplayType::Block)
                .margin(BoxDimension {
                    left: Dimension::Pixels(0.),
                    right: Dimension::Pixels(0.),
                    top: Dimension::Pixels(2.),
                    bottom: Dimension::Pixels(0.),
                }),
        );
    }

    let hover_colors = if !is_active {
        Some(ElementColors {
            border: BorderColor::default(),
            bg: sidebar_card_hover().into(),
            text: InheritableColor::Inherited,
        })
    } else {
        None
    };

    // card_width minus margin (6+6=12) to get inner width
    let inner_width = (card_width - 12.0).max(1.0);

    // Left accent bar: 3px blue on active, transparent on inactive
    let border_color = if is_active {
        BorderColor {
            left: sidebar_accent(),
            top: LinearRgba::TRANSPARENT,
            right: LinearRgba::TRANSPARENT,
            bottom: LinearRgba::TRANSPARENT,
        }
    } else {
        BorderColor::default()
    };
    let border_width = if is_active {
        BoxDimension {
            left: Dimension::Pixels(3.),
            top: Dimension::Pixels(0.),
            right: Dimension::Pixels(0.),
            bottom: Dimension::Pixels(0.),
        }
    } else {
        BoxDimension::new(Dimension::Pixels(0.))
    };

    let card_corners = None;

    Element::new(font, ElementContent::Children(card_children))
        .display(DisplayType::Block)
        .item_type(UIItemType::SidebarWorkspace(entry.name.clone()))
        .min_width(Some(Dimension::Pixels(inner_width)))
        .padding(BoxDimension {
            left: Dimension::Pixels(12.),
            right: Dimension::Pixels(12.),
            top: Dimension::Pixels(10.),
            bottom: Dimension::Pixels(10.),
        })
        .margin(BoxDimension {
            left: Dimension::Pixels(6.),
            right: Dimension::Pixels(6.),
            top: Dimension::Pixels(2.),
            bottom: Dimension::Pixels(2.),
        })
        .border(border_width)
        .border_corners(card_corners)
        .colors(ElementColors {
            border: border_color,
            bg: card_bg.into(),
            text: InheritableColor::Inherited,
        })
        .hover_colors(hover_colors)
}

impl crate::TermWindow {
    pub fn paint_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> anyhow::Result<()> {
        let sidebar_width = self.sidebar_pixel_width();
        if sidebar_width <= 0.0 {
            return Ok(());
        }

        let border = self.get_os_border();
        let sidebar_x = border.left.get() as f32;
        let sidebar_y = border.top.get() as f32;
        let sidebar_height =
            self.dimensions.pixel_height as f32 - sidebar_y - border.bottom.get() as f32;

        if sidebar_height <= 0.0 {
            return Ok(());
        }

        let handle_width = 6.0_f32;

        // Background fill (layer 0)
        self.filled_rectangle(
            layers,
            0,
            euclid::rect(sidebar_x, sidebar_y, sidebar_width, sidebar_height),
            sidebar_bg(),
        )?;

        // Right edge separator (layer 2)
        self.filled_rectangle(
            layers,
            2,
            euclid::rect(
                sidebar_x + sidebar_width - 1.0,
                sidebar_y,
                1.0,
                sidebar_height,
            ),
            sidebar_separator(),
        )?;

        // Resize handle hit region
        self.ui_items.push(UIItem {
            x: (sidebar_x + sidebar_width - handle_width) as usize,
            y: sidebar_y as usize,
            width: handle_width as usize,
            height: sidebar_height as usize,
            item_type: UIItemType::SidebarResizeHandle,
        });

        // Title: bold Roboto 12pt. Body/meta: regular Roboto 11pt (1pt smaller).
        let font = self.fonts.title_font()?;
        let body_font = self.fonts.sidebar_body_font()?;
        let mono_font = Rc::clone(&body_font);
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let cell_width = metrics.cell_size.width as f32;

        // Available text width inside cards (sidebar - margins - padding - handle)
        let content_width = sidebar_width - handle_width;
        let text_width = (content_width - 6.0 * 2.0 - 10.0 * 2.0).max(cell_width);
        let text_cols = (text_width / cell_width).floor().max(1.0) as usize;

        // Build Element tree
        let mut root_children: Vec<Element> = vec![];

        // Toolbar row: [+] [bell] [split-h] [split-v]
        let toolbar_button = |font: &Rc<LoadedFont>,
                              label: &str,
                              item_type: UIItemType|
         -> Element {
            Element::new(font, ElementContent::Text(label.to_string()))
                .item_type(item_type)
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: InheritableColor::Inherited,
                    text: sidebar_muted().into(),
                })
                .hover_colors(Some(ElementColors {
                    border: BorderColor::default(),
                    bg: LinearRgba::with_srgba(255, 255, 255, 25).into(),
                    text: sidebar_text().into(),
                }))
                .padding(BoxDimension {
                    left: Dimension::Pixels(6.),
                    right: Dimension::Pixels(6.),
                    top: Dimension::Pixels(4.),
                    bottom: Dimension::Pixels(4.),
                })
        };

        let toolbar_items: Vec<Element> = vec![
            toolbar_button(&font, "\u{25EB}", UIItemType::SidebarSplitHorizontal), // ◫ split left|right
            toolbar_button(&font, "\u{229F}", UIItemType::SidebarSplitVertical),   // ⊟ split top/bottom
            toolbar_button(&font, "\u{237E}", UIItemType::SidebarNotificationBell), // ⍾ bell
            toolbar_button(&font, "+", UIItemType::SidebarNewWorkspace),
        ];

        root_children.push(
            Element::new(&font, ElementContent::Children(toolbar_items))
                .display(DisplayType::Block)
                .padding(BoxDimension {
                    left: Dimension::Pixels(8.),
                    right: Dimension::Pixels(8.),
                    top: Dimension::Pixels(6.),
                    bottom: Dimension::Pixels(4.),
                })
                .border(BoxDimension {
                    left: Dimension::Pixels(0.),
                    right: Dimension::Pixels(0.),
                    top: Dimension::Pixels(0.),
                    bottom: Dimension::Pixels(1.),
                })
                .colors(ElementColors {
                    border: BorderColor {
                        top: LinearRgba::TRANSPARENT,
                        bottom: sidebar_separator(),
                        left: LinearRgba::TRANSPARENT,
                        right: LinearRgba::TRANSPARENT,
                    },
                    bg: InheritableColor::Inherited,
                    text: InheritableColor::Inherited,
                }),
        );

        // Workspace cards
        let entries = self.sidebar_entries();
        for entry in &entries {
            root_children.push(build_card_element(&font, &body_font, &mono_font, entry, text_cols, content_width));
        }

        // Footer: settings gear
        root_children.push(
            Element::new(&font, ElementContent::Text("\u{2699}".to_string()))
                .display(DisplayType::Block)
                .item_type(UIItemType::SidebarSettings)
                .padding(BoxDimension {
                    left: Dimension::Pixels(12.),
                    right: Dimension::Pixels(12.),
                    top: Dimension::Pixels(8.),
                    bottom: Dimension::Pixels(8.),
                })
                .border(BoxDimension {
                    left: Dimension::Pixels(0.),
                    right: Dimension::Pixels(0.),
                    top: Dimension::Pixels(1.),
                    bottom: Dimension::Pixels(0.),
                })
                .colors(ElementColors {
                    border: BorderColor {
                        top: sidebar_separator(),
                        bottom: LinearRgba::TRANSPARENT,
                        left: LinearRgba::TRANSPARENT,
                        right: LinearRgba::TRANSPARENT,
                    },
                    bg: InheritableColor::Inherited,
                    text: sidebar_muted().into(),
                })
                .hover_colors(Some(ElementColors {
                    border: BorderColor {
                        top: sidebar_separator(),
                        bottom: LinearRgba::TRANSPARENT,
                        left: LinearRgba::TRANSPARENT,
                        right: LinearRgba::TRANSPARENT,
                    },
                    bg: LinearRgba::with_srgba(255, 255, 255, 18).into(),
                    text: sidebar_text().into(),
                })),
        );

        let root = Element::new(&font, ElementContent::Children(root_children))
            .display(DisplayType::Block)
            .min_width(Some(Dimension::Pixels(content_width.max(1.0))))
            .min_height(Some(Dimension::Pixels(sidebar_height)))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: sidebar_bg().into(),
                text: sidebar_text().into(),
            });

        let gl_state = self.render_state.as_ref().unwrap();
        let mut computed = self.compute_element(
            &LayoutContext {
                height: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: sidebar_height,
                    pixel_cell: metrics.cell_size.height as f32,
                },
                width: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: content_width,
                    pixel_cell: metrics.cell_size.width as f32,
                },
                bounds: euclid::rect(0., 0., content_width.max(1.0), sidebar_height),
                metrics: &metrics,
                gl_state,
                zindex: 10,
            },
            &root,
        )?;

        // Clamp scroll offset: content height minus visible height, minimum 0
        let content_height = computed.bounds.height();
        let max_scroll = (content_height - sidebar_height).max(0.0);
        self.sidebar.scroll_offset = self.sidebar.scroll_offset.clamp(0.0, max_scroll);
        let scroll_offset = self.sidebar.scroll_offset;

        // Translate to sidebar position, applying scroll offset
        computed.translate(euclid::vec2(sidebar_x, sidebar_y - scroll_offset));

        // Render via box_model
        self.render_element(&computed, gl_state, None)?;

        // Background hit region FIRST (lowest priority — pushed before card items)
        // Hit-testing iterates in reverse, so items pushed later win.
        self.ui_items.push(UIItem {
            x: sidebar_x as usize,
            y: sidebar_y as usize,
            width: (content_width) as usize,
            height: sidebar_height as usize,
            item_type: UIItemType::SidebarBackground,
        });

        // Card + button UI items on top (higher priority for clicks)
        for item in computed.ui_items() {
            self.ui_items.push(item);
        }

        Ok(())
    }
}

fn truncate_to_cells(text: &str, cols: usize) -> String {
    if text.chars().count() <= cols {
        return text.to_string();
    }
    if cols <= 3 {
        return text.chars().take(cols).collect();
    }

    let mut truncated = text
        .chars()
        .take(cols.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn wrap_text_to_cells(text: &str, cols: usize, max_lines: usize) -> Vec<String> {
    if text.is_empty() || cols == 0 || max_lines == 0 {
        return vec![];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![];
    }

    let mut lines = vec![];
    let mut current = String::new();

    for word in words {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };

        if candidate.chars().count() <= cols {
            current = candidate;
            continue;
        }

        if current.is_empty() {
            lines.push(truncate_to_cells(word, cols));
            continue;
        }

        lines.push(current);
        current = word.to_string();
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.len() <= max_lines {
        return lines;
    }

    let mut visible = lines[..max_lines.saturating_sub(1)].to_vec();
    let remaining = lines[max_lines - 1..].join(" ");
    visible.push(truncate_to_cells(&remaining, cols));
    visible
}

fn sidebar_pull_request_color(status: WorkspacePullRequestStatus, is_active: bool) -> LinearRgba {
    if is_active {
        // cmux: white at 75% opacity on active card
        return LinearRgba::with_srgba(255, 255, 255, 191);
    }
    // cmux: .secondary color for inactive
    match status {
        WorkspacePullRequestStatus::Open => sidebar_pull_request_open(),
        WorkspacePullRequestStatus::Merged => sidebar_pull_request_merged(),
        WorkspacePullRequestStatus::Closed => sidebar_pull_request_closed(),
    }
}

#[cfg(test)]
mod test {
    use super::{
        compact_components, format_listening_ports, sidebar_entry_body_lines,
        sidebar_pull_request_text, wrap_text_to_cells, SidebarLine, SidebarLineStyle,
    };
    use crate::termwindow::sidebar::{
        WorkspaceEntry, WorkspacePullRequest, WorkspacePullRequestStatus,
    };
    use std::path::Path;

    #[test]
    fn compact_components_preserve_tail_context() {
        assert_eq!(
            compact_components(Path::new("code/wezmux/sidebar"), Some("~"), 3),
            "~/code/wezmux/sidebar".to_string()
        );
    }

    #[test]
    fn listening_ports_are_compact() {
        assert_eq!(
            format_listening_ports(&[3000, 5173, 8080, 9000]),
            Some(":3000, :5173, :8080 +1".to_string())
        );
    }

    #[test]
    fn notification_preview_wraps_before_meta() {
        let entry = WorkspaceEntry {
            name: "alpha".to_string(),
            title: "Claude Code".to_string(),
            cwd: Some("wezmux".to_string()),
            cwd_path: Some(Path::new("/tmp/wezmux").to_path_buf()),
            git_branch: Some("main".to_string()),
            git_dirty: true,
            listening_ports: vec![3000],
            pull_request: Some(WorkspacePullRequest {
                number: 704,
                status: WorkspacePullRequestStatus::Open,
            }),
            latest_notification: Some(
                "Confirms it the file in the bucket is truncated unexpectedly".to_string(),
            ),
            unread_count: 1,
            tab_count: 2,
            pane_count: 4,
            is_active: false,
            is_hovered: false,
            agent: None,
        };

        assert_eq!(
            sidebar_entry_body_lines(&entry, 28),
            vec![
                SidebarLine {
                    text: "Confirms it the file in the".to_string(),
                    style: SidebarLineStyle::Preview,
                },
                SidebarLine {
                    text: "bucket is truncated unexp...".to_string(),
                    style: SidebarLineStyle::Preview,
                },
                SidebarLine {
                    text: "main* \u{2022} /tmp/wezmux".to_string(),
                    style: SidebarLineStyle::Meta,
                },
                SidebarLine {
                    text: "◦ PR #704 open".to_string(),
                    style: SidebarLineStyle::PullRequest(WorkspacePullRequestStatus::Open),
                },
                SidebarLine {
                    text: ":3000".to_string(),
                    style: SidebarLineStyle::Secondary,
                }
            ]
        );
    }

    #[test]
    fn wrap_text_to_cells_adds_ellipsis_when_truncated() {
        assert_eq!(
            wrap_text_to_cells("alpha beta gamma delta epsilon", 12, 2),
            vec!["alpha beta".to_string(), "gamma del...".to_string()]
        );
    }

    #[test]
    fn pull_request_text_is_compact() {
        assert_eq!(
            sidebar_pull_request_text(&WorkspacePullRequest {
                number: 680,
                status: WorkspacePullRequestStatus::Merged,
            }),
            "✓ PR #680 merged".to_string()
        );
    }
}
