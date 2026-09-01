package registry

import (
	"path/filepath"
	"sort"
	"sync"
	"time"

	"claude-status/internal/feed"
)

type Status int

const (
	StatusIdle Status = iota
	StatusRunning
	StatusAwaiting
	StatusEnded
)

var (
	statusName  = [...]string{"idle", "running", "awaiting", "ended"}
	statusGlyph = [...]string{"○", "●", "◐", "·"}
)

func (s Status) String() string { return statusName[s] }
func (s Status) Glyph() string  { return statusGlyph[s] }

// statusFromFeed maps a feed status string to the UI status enum.
func statusFromFeed(s string) Status {
	switch s {
	case feed.StatusRunning:
		return StatusRunning
	case feed.StatusAwaiting:
		return StatusAwaiting
	default:
		return StatusIdle
	}
}

type Session struct {
	ID             string
	Cwd            string
	ProjectDir     string
	TranscriptPath string
	Status         Status
	LastTool       string
	LastEvent      time.Time
	Summary        string
	SummaryAt      time.Time
	Dirty          bool
}

type Registry struct {
	mu       sync.RWMutex
	sessions map[string]*Session
}

func New() *Registry {
	return &Registry{sessions: map[string]*Session{}}
}

// Reconcile folds the current session feed (written by the `claude-status hook`)
// into the registry and returns the sessions whose state advanced since we last
// saw them (callers use that to (re-)request a summary). Sessions absent from
// the feed are marked ended — the hook removes a session's file on SessionEnd,
// and Load() drops stale ones. Status comes straight from the hook events
// (running / awaiting / idle), so it's accurate, not inferred.
func (r *Registry) Reconcile(states []feed.State) []*Session {
	r.mu.Lock()
	defer r.mu.Unlock()

	seen := make(map[string]struct{}, len(states))
	var changed []*Session

	for _, st := range states {
		if st.SessionID == "" {
			continue
		}
		seen[st.SessionID] = struct{}{}

		s, ok := r.sessions[st.SessionID]
		if !ok {
			s = &Session{ID: st.SessionID}
			r.sessions[st.SessionID] = s
		}

		advanced := st.UpdatedAt.After(s.LastEvent)

		s.Cwd = st.Cwd
		s.ProjectDir = filepath.Base(st.Cwd)
		s.TranscriptPath = st.TranscriptPath
		if st.LastTool != "" {
			s.LastTool = st.LastTool
		}
		s.LastEvent = st.UpdatedAt
		s.Status = statusFromFeed(st.Status)

		// First sighting, or new activity since our last summary. Need a
		// transcript path to summarise from.
		if (advanced || s.SummaryAt.IsZero()) && s.TranscriptPath != "" {
			s.Dirty = true
			changed = append(changed, s)
		}
	}

	// Anything we knew about but the feed no longer reports has ended.
	for id, s := range r.sessions {
		if _, ok := seen[id]; !ok {
			s.Status = StatusEnded
		}
	}

	return changed
}

func (r *Registry) SetSummary(id, summary string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	s, ok := r.sessions[id]
	if !ok {
		return
	}
	s.Summary = summary
	s.SummaryAt = time.Now()
	s.Dirty = false
}

// Snapshot returns a copy of active (non-ended) sessions, sorted most-recent-first.
func (r *Registry) Snapshot() []*Session {
	r.mu.RLock()
	defer r.mu.RUnlock()
	out := make([]*Session, 0, len(r.sessions))
	for _, s := range r.sessions {
		if s.Status == StatusEnded {
			continue
		}
		cp := *s
		out = append(out, &cp)
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].LastEvent.After(out[j].LastEvent)
	})
	return out
}
