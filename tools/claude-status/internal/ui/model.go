package ui

import (
	"fmt"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/mattn/go-runewidth"

	"claude-status/internal/registry"
	"claude-status/internal/summarizer"
)

type tickMsg time.Time

type col struct {
	title string
	width int
	fixed bool
}

type Model struct {
	reg      *registry.Registry
	sum      *summarizer.Summarizer
	cols     []col
	cursor   int
	width    int
	height   int
	sessions []*registry.Session
	quitting bool
}

var (
	colAccent   = lipgloss.Color("#CBA6F7")
	colSubtle   = lipgloss.Color("#9399B2")
	colFaint    = lipgloss.Color("#6C7086")
	colRunning  = lipgloss.Color("#A6E3A1")
	colAwaiting = lipgloss.Color("#F9E2AF")
	colIdle     = lipgloss.Color("#6C7086")
	colEnded    = lipgloss.Color("#585B70")
	colTool     = lipgloss.Color("#89B4FA")
	colProject  = lipgloss.Color("#CDD6F4")
	colSelectFg = lipgloss.Color("#1E1E2E")
	colSelectBg = lipgloss.Color("#CBA6F7")
	colHeaderFg = lipgloss.Color("#F5C2E7")

	titleStyle    = lipgloss.NewStyle().Bold(true).Foreground(colAccent).Padding(0, 1)
	titleCountSty = lipgloss.NewStyle().Foreground(colSubtle)
	helpStyle     = lipgloss.NewStyle().Faint(true).Foreground(colSubtle).Padding(0, 1)
	headerStyle   = lipgloss.NewStyle().Bold(true).Foreground(colHeaderFg)
	sepStyle      = lipgloss.NewStyle().Foreground(colFaint)
	projectStyle  = lipgloss.NewStyle().Bold(true).Foreground(colProject)
	toolStyle     = lipgloss.NewStyle().Foreground(colTool)
	idleStyle     = lipgloss.NewStyle().Foreground(colFaint)
	summaryStyle  = lipgloss.NewStyle().Foreground(colProject)
	selectedStyle = lipgloss.NewStyle().Background(colSelectBg).Foreground(colSelectFg).Bold(true)
	emptyStyle    = lipgloss.NewStyle().Faint(true).Foreground(colSubtle).Padding(1, 2)

	statusStyles = map[registry.Status]lipgloss.Style{
		registry.StatusRunning:  lipgloss.NewStyle().Bold(true).Foreground(colRunning),
		registry.StatusAwaiting: lipgloss.NewStyle().Bold(true).Foreground(colAwaiting),
		registry.StatusIdle:     lipgloss.NewStyle().Foreground(colIdle),
		registry.StatusEnded:    lipgloss.NewStyle().Foreground(colEnded),
	}
)

func NewModel(reg *registry.Registry, sum *summarizer.Summarizer) *Model {
	m := &Model{
		reg:    reg,
		sum:    sum,
		width:  120,
		height: 30,
	}
	m.relayout()
	return m
}

func (m *Model) Init() tea.Cmd { return tick() }

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.relayout()

	case tea.KeyMsg:
		switch msg.String() {
		case "q", "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		case "up", "k":
			if m.cursor > 0 {
				m.cursor--
			}
		case "down", "j":
			if m.cursor < len(m.sessions)-1 {
				m.cursor++
			}
		case "g", "home":
			m.cursor = 0
		case "G", "end":
			m.cursor = max(0, len(m.sessions)-1)
		}

	case tickMsg:
		return m, tick()
	}
	return m, nil
}

func (m *Model) View() string {
	if m.quitting {
		return ""
	}
	m.sessions = m.reg.Snapshot()
	if m.cursor >= len(m.sessions) {
		m.cursor = max(0, len(m.sessions)-1)
	}

	title := titleStyle.Render("claude-status") +
		titleCountSty.Render(fmt.Sprintf("  %d active", len(m.sessions)))

	header := m.renderHeader()
	sep := sepStyle.Render(strings.Repeat("─", m.tableWidth()))

	var body string
	if len(m.sessions) == 0 {
		body = emptyStyle.Render("no claude sessions running — start one in any terminal")
	} else {
		rows := make([]string, len(m.sessions))
		for i, s := range m.sessions {
			rows[i] = m.renderRow(s, i == m.cursor)
		}
		body = strings.Join(rows, "\n")
	}

	help := helpStyle.Render("q quit  ·  ↑↓ navigate  ·  g/G top/bottom")
	return strings.Join([]string{title, header, sep, body, help}, "\n")
}

const colGap = 2 // spaces between columns

func (m *Model) tableWidth() int {
	w := 0
	for i, c := range m.cols {
		w += c.width
		if i < len(m.cols)-1 {
			w += colGap
		}
	}
	return w
}

func (m *Model) relayout() {
	projW, statusW, idleW, toolW := 24, 12, 7, 12
	gaps := colGap * 4
	sumW := m.width - projW - statusW - idleW - toolW - gaps - 2
	if sumW < 20 {
		sumW = 20
	}
	m.cols = []col{
		{title: "project", width: projW, fixed: true},
		{title: "status", width: statusW, fixed: true},
		{title: "idle", width: idleW, fixed: true},
		{title: "tool", width: toolW, fixed: true},
		{title: "summary", width: sumW},
	}
}

func (m *Model) renderHeader() string {
	parts := make([]string, len(m.cols))
	for i, c := range m.cols {
		parts[i] = headerStyle.Render(padRight(c.title, c.width))
	}
	return strings.Join(parts, strings.Repeat(" ", colGap))
}

func (m *Model) renderRow(s *registry.Session, selected bool) string {
	idle := time.Since(s.LastEvent).Round(time.Second)
	tool := s.LastTool
	if tool == "" {
		tool = "—"
	}

	cells := []string{
		styleCell(projectStyle, s.ProjectDir, m.cols[0].width),
		styleCell(statusStyles[s.Status], s.Status.Glyph()+" "+s.Status.String(), m.cols[1].width),
		styleCell(idleStyle, formatIdle(idle), m.cols[2].width),
		styleCell(toolStyle, tool, m.cols[3].width),
		styleCell(summaryStyle, s.Summary, m.cols[4].width),
	}
	row := strings.Join(cells, strings.Repeat(" ", colGap))
	if selected {
		return selectedStyle.Render(row)
	}
	return row
}

// styleCell truncates `text` to visible width `w` (ANSI-safe), applies `style`,
// then pads the rendered string with spaces so the cell occupies exactly `w` columns.
func styleCell(style lipgloss.Style, text string, w int) string {
	t := truncatePlain(text, w)
	rendered := style.Render(t)
	pad := w - runewidth.StringWidth(t)
	if pad > 0 {
		rendered += strings.Repeat(" ", pad)
	}
	return rendered
}

func truncatePlain(s string, w int) string {
	if runewidth.StringWidth(s) <= w {
		return s
	}
	if w <= 1 {
		return "…"
	}
	return runewidth.Truncate(s, w, "…")
}

func padRight(s string, w int) string {
	visible := runewidth.StringWidth(s)
	if visible >= w {
		return runewidth.Truncate(s, w, "…")
	}
	return s + strings.Repeat(" ", w-visible)
}

func tick() tea.Cmd {
	return tea.Tick(time.Second, func(t time.Time) tea.Msg { return tickMsg(t) })
}

func formatIdle(d time.Duration) string {
	if d < time.Minute {
		return fmt.Sprintf("0:%02d", int(d.Seconds()))
	}
	mn := int(d.Minutes())
	s := int(d.Seconds()) - mn*60
	if mn < 60 {
		return fmt.Sprintf("%d:%02d", mn, s)
	}
	h := mn / 60
	mn -= h * 60
	return fmt.Sprintf("%dh%02d", h, mn)
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
