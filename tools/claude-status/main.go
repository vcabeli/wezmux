package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"claude-status/internal/feed"
	"claude-status/internal/registry"
	"claude-status/internal/summarizer"
	"claude-status/internal/ui"
)

// pollInterval is how often we re-read the session feed written by the hook.
const pollInterval = 1 * time.Second

func main() {
	// `claude-status hook`: invoked by Claude Code hooks (see README). Reads one
	// event from stdin, records session state, exits. Must run before anything
	// that would start the TUI.
	if len(os.Args) > 1 && os.Args[1] == "hook" {
		feed.RunHook(os.Stdin)
		return
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sigCh
		cancel()
	}()

	reg := registry.New()
	sum := summarizer.New()

	// reconcile re-reads the session feed and (re-)summarises any that advanced.
	reconcile := func() {
		for _, s := range reg.Reconcile(feed.Load()) {
			sid, tp := s.ID, s.TranscriptPath
			sum.Request(sid, tp, func(r summarizer.Result) {
				if r.Err == nil {
					reg.SetSummary(r.SessionID, r.Summary)
				}
			})
		}
	}

	// Seed once before the UI paints so the table isn't empty on the first frame.
	reconcile()

	go func() {
		ticker := time.NewTicker(pollInterval)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				reconcile()
			}
		}
	}()

	model := ui.NewModel(reg, sum)
	p := tea.NewProgram(model, tea.WithAltScreen())
	if _, err := p.Run(); err != nil {
		log.Fatal(err)
	}
}
