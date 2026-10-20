# Inline Diagrams

> Status: exploration (draft PR). Mermaid only, macOS GPU renderer only.

When an agent writes Mermaid source between `<wezmux_diagram>` tags, Wezmux draws the diagram where the source text would have been. The diagram uses the terminal's own font, size and colors, and it is redrawn whenever the pane is resized.

```
<wezmux_diagram>
flowchart LR
  A[Hook script] --> B[OSC parser]
  B --> C{AgentStatusStore}
  C -->|generation++| D[Sidebar]
</wezmux_diagram>
```

The agent needs one line of prompt to know this is available. The text lives in `bin/hooks/lib/diagram-prompt.sh`:

> You are running inside Wezmux, which draws Mermaid diagrams inline. When a diagram would help, put Mermaid source between a line `<wezmux_diagram>` and a line `</wezmux_diagram>` instead of a code fence. It is drawn in the rows its source occupies, so keep it compact and prefer left-to-right layouts (flowchart LR).

- **Claude Code.** The `claude` wrapper appends it through `--append-system-prompt`. It skips it for `-p`/`--print`, because printed output may be piped elsewhere, and when the caller passes its own `--system-prompt` or `--append-system-prompt`.
- **Codex.** A `SessionStart` hook (`bin/hooks/codex/on-session-start.sh`, installed by `make install-codex-hooks`) returns it as `additionalContext`. Codex asks you to trust the new hook once. The hook only has its environment to go on. A Codex attached to the app-server daemon runs hooks with the environment of the pane that started the daemon, so a daemon started in Wezmux also tells sessions running in other terminals.

Set `WEZMUX_DIAGRAMS=0` to leave it out for both.

## Why a tag, and not a ```` ```mermaid ```` fence

Wezmux only sees what the agent's TUI prints into the grid. Measured with Claude Code 2.1.304 and Codex in tmux:

| Agent | ```` ```mermaid ```` fence | `<wezmux_diagram>` lines |
|---|---|---|
| Claude Code | Fence stripped; only the word `mermaid` and the source remain | Printed verbatim, indented 2 columns |
| Codex | Drawn as box art by Codex's own text-mode renderer; larger diagrams fall back to *"This Mermaid diagram exceeds the terminal renderer's size limits."* plus the source | Printed verbatim (indentation inside dropped, which Mermaid ignores) |

A tag on its own line survives both TUIs. Wezmux also recognizes Codex's "exceeds the size limits" fallback, so large Codex diagrams render even without the prompt line.

## What this adds over the agents themselves

- **Claude Code** does not render diagrams in the terminal. Models fall back to hand-drawn ASCII art, which breaks when the pane is narrower than the model assumed and costs output tokens.
- **Codex** renders small Mermaid diagrams as box-drawing characters and gives up on larger ones (see the fallback above), which is where a diagram is most useful.
- The desktop and web apps render Mermaid, but in their own GUI, not in the terminal where the agent runs.

## Prior art in terminals

No mainstream terminal renders diagram source by itself. Existing tools render Mermaid to a PNG (usually through mermaid-cli and headless Chrome) and send it with the kitty or iTerm2 image protocol:

- [pi-mermaid](https://github.com/azzz9/pi-mermaid) (Pi extension) and [kit](https://github.com/akonwi/kit/issues/95): agent-side, PNG over an image protocol.
- [mermaid-ascii](https://news.ycombinator.com/item?id=46804828) and [mmaid-go](https://github.com/aaronsb/mmaid-go): text-mode rendering, the same family as Codex's renderer.
- kitty's Unicode-placeholder image protocol (U+10EEEE) is designed to survive TUIs and tmux. WezTerm doesn't implement placeholders, and the agent would still need a side channel to send the image.

Those approaches either need the agent to emit escape sequences (TUIs strip them) or give up on resolution. Rendering on the terminal side avoids both problems.

## How it works

```
pane text ──scan──▶ DiagramBlock{rows, col, source}
                          │ (once per source × box size × palette/font)
                          ▼
     background thread: mermaid-rs-renderer → SVG → usvg/resvg → RGBA
                          │ TermWindowNotif::Apply + invalidate
                          ▼
     paint_pane: skip the block's rows, draw one textured quad over them
```

All of this lives in `wezterm-gui/src/termwindow/diagram.rs`. `render/pane.rs` has about 20 lines of hooks, and `LoadedFont::primary_face` in `wezterm-font` is about 10 lines.

- **Detection.** `find_blocks` scans the viewport ± 200 rows, and the scan is cached per pane on the pane's sequence number. A block is an opening tag alone on its line (after indentation or a `⏺`/`•` message bullet), its source, and a closing tag alone on its line. Prose that mentions the tag stays text. Soft-wrapped rows are joined first. An unclosed block (the agent is still streaming) stays as text until the closing tag arrives.
- **Rendering.** [mermaid-rs-renderer](https://github.com/1jehuang/mermaid-rs-renderer) handles parsing and layout in pure Rust, without a browser, and resvg rasterizes the SVG at the exact device-pixel size of the box. The natural layout is tried first, then a layout hinted toward the box's aspect ratio, and whichever is larger wins. Diagrams are never drawn larger than the terminal's text size. If the result would need scaling below 50%, the source stays as text.
- **Flush look.** There are no cards or panels. The background is transparent, and node fills are the foreground blended into the background at 3–10%. Borders and edges are the foreground at 45–55%, text is the terminal foreground, and accents (pie slices, git branches) are the ANSI palette. Text uses the terminal's resolved primary font, including the bundled JetBrains Mono, which isn't installed system-wide, at its exact pixel size. The diagram is left-aligned with the tag's column and vertically centered in the block's rows.
- **Drawing.** Rows under a rendered block are not painted. The diagram is one textured quad, clipped to the viewport, so it scrolls with the text in the same frame. While a re-render for a new size is in flight, the previous render is shrunk to fit, so resizing doesn't flash the source text.
- **Selection.** The source is still in the grid, so selecting over a diagram copies its Mermaid source.

## Limits and open questions

- **The rows belong to the TUI.** The diagram must fit in the rows its source occupies. LR flowcharts and short sequence diagrams fit well. A tall TD chart with long labels does not: the 10-node Codex example needs about twice the rows it has, so it stays as text. Options:
  1. Hover or click shows the full-size diagram in an overlay anchored at the block, drawn on the modal layer in the same flush style. This is the most promising.
  2. Have the prompt ask the model to pad the block with `%%` comment lines to reserve height.
  3. Accept smaller text for tall diagrams.
- **Dependency weight.** Wezmux's own code is about 650 lines plus tests. The renderer crate is about 58k lines (usvg about 19k, resvg about 5k), adding 29 crates to the lock file, all pure Rust and MIT/Apache. Release binary growth is not measured yet. If that is too much, the same module could call an `mmdr` binary out-of-process instead.
- **Renderer fidelity.** mermaid-rs-renderer is in early development and does not match mermaid-js in every case. Source it can't parse stays text.
- **Other agents.** Oh My Pi doesn't get the prompt line yet. Its Wezmux extension could add it.
- **Config.** There is no `config.diagrams` toggle yet; detection is always on, and only an explicit tag triggers it.
