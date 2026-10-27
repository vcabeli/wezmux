# Inline Math

> Status: exploration. LaTeX via RaTeX, macOS GPU renderer only. Builds on [inline diagrams](inline-diagrams.md).

When an agent prints LaTeX between `$…$` or `$$…$$`, Wezmux draws the formula where its source was. Inline math sits on the text baseline, and the rest of the line closes up after it. Display math takes the rows of its `$$` block. Math is drawn in the terminal's foreground color and scaled to the terminal's text size, and it is redrawn when the pane is resized.

```
⏺ The kinetic energy is `$E_k = \frac{1}{2} m v^2$`.

  $$
  \sum_{i=1}^{n} i = \frac{n(n+1)}{2}
  $$
```

Everything runs locally, in-process. The math is laid out by [RaTeX](https://github.com/erweixin/RaTeX), a pure-Rust port of KaTeX with KaTeX's fonts compiled in, and rasterized by resvg, which diagrams already use. Nothing goes over the network, and no JavaScript or browser is involved.

## What is recognized

- **Display math.** A line holding only `$$`, the formula, and a line holding only `$$`, at most 40 rows apart. A line holding only `$$…$$` also counts. Indentation and agent message bullets (`⏺`, `•`) before the delimiter are allowed. Display style is used. If that doesn't fit the rows at 75% or more, as with a one-row `$$\int…$$`, the flatter inline style is tried and the larger result wins.
- **Inline math.** `$…$` (or `$$…$$` mid-line) within one row. Delimiters follow pandoc: the opening `$` is followed by a non-space and not preceded by a letter or digit, the closing `$` is preceded by a non-space and not followed by a letter or digit, and `\$` is a literal dollar. The content must also look like math: an operator or `\`/`^`/`_`/`{`, or a short term such as `n`, `x'` or `f(x)`. So `$5 and $10`, `"$HOME" "$PATH"`, `$PATH:$HOME` and awk's `$1 + $2` stay text.
- **Not on the cursor row**, which may be a prompt being typed into, and not in full-screen apps on the alternate screen (editors) unless an agent runs in the pane. Codex draws on the alternate screen.
- A formula that RaTeX can't parse, or that would need scaling below 50% to fit, stays text.

## Why the prompt asks for backticks and a code fence

Wezmux only sees what the agent's TUI prints. Measured with Claude Code 2.1.311 and Codex in tmux, the same model reply looks like this:

| Model wrote | Claude Code prints | Codex prints |
|---|---|---|
| `$E = mc^2$` in prose | verbatim | `E = mc²` (its own Unicode rendering) |
| `$a \, b \{x\}$` in prose | `$a , b {x}$` (Markdown backslash escapes) | verbatim |
| `$$` block containing `x^{2}_{3} * y^{4}_{5}` and `\\` | `x^{2}{3} * y^{4}{5}` (`_…_` became italics) and `\` | verbatim (multi-line), 2-D box art (one-line `$$…$$`) |
| `` `$\{x\} \\ a_{1} * b_{2}$` `` | `$\{x\} \\ a_{1} * b_{2}$` (verbatim, code-colored) | verbatim |
| ```` ```latex ```` fence around a `$$` block | the `$$` block verbatim, fence stripped | the `$$` block verbatim, fence stripped |

Bare math loses backslashes and underscores in Claude Code and is pre-rendered as text art by Codex. Inside a code span or a fenced block it survives both. The prompt (in `bin/hooks/lib/inline-prompt.sh`, sent the same way as the diagram prompt) asks for exactly that:

> Write inline math as $...$ inside backticks, like `$E = mc^2$`, and display math as a ```latex code block holding a line $$, the formula, and a line $$. Without the backticks or the code block, the terminal's Markdown rendering strips backslashes and underscores. A formula is drawn in the rows its source occupies, so put each row of a matrix or aligned environment on its own line.

Detection doesn't depend on the prompt. Bare `$…$` that does reach the grid (from `cat`, other agents, or simple formulas that Markdown leaves alone) is drawn too. Set `WEZMUX_MATH=0` to leave the math part of the prompt out. Math detection itself is always on.

## How it works

```
pane text ──scan──▶ DiagramBlock{kind: DisplayMath, rows, col, source}   (with Mermaid blocks)
           └──────▶ InlineMath{row, cols, source}
                          │ (once per source × box size × palette/font)
                          ▼
     background thread: RaTeX parse → layout → SVG (glyphs as paths) → resvg → RGBA + baseline
                          │ TermWindowNotif::Apply + invalidate
                          ▼
     paint_pane: blocks hide their rows; inline rows are painted from a collapsed copy,
                 then each formula is one textured quad
```

Detection and rendering live in `wezterm-gui/src/termwindow/math.rs`. The scan cache, render cache, render thread and quad painting are shared with `diagram.rs`, which now has a `RenderKind` per render. `render/pane.rs` has about 30 more lines of hooks.

- **Size.** KaTeX sets math at 1.21× the surrounding text so the x-heights match. Here the terminal's font size stands in for that text, and the formula is never drawn larger than it. Glyphs are laid out at the final size, so they are rasterized crisp rather than scaled.
- **Inline layout.** The grid is the TUI's, so the formula can't take cells of its own. Once a formula is rendered, its row is painted from a copy of the line in which the `$…$` cells are replaced by just enough blank cells to hold the formula (`collapse_line`), so the text after it closes up. The blanks keep the surrounding background. The copy is cached per row and line sequence number, so its shape hash survives across frames. The selection on that row is remapped to the collapsed columns. The formula is drawn on the font's baseline (`cell_height + descender`), clamped into the row.
- **Display layout.** The same as diagrams: rows under the block aren't painted, and the formula is left-aligned with the `$$` and vertically centered in the rows.
- **Selecting.** A formula or block that the selection touches is drawn as its source until the selection clears, so what is highlighted is what gets copied (its LaTeX). The rest stay drawn.
- **Mouse.** Each pane's painted inline math is kept, and the mouse handler maps the pointer's column back through the collapse (`math::grid_col`), so clicks, drags, hyperlink hover and mouse reporting land on the grid cell drawn under the pointer.

## Limits and open questions

- **Rows belong to the TUI.** As with diagrams, display math has only the rows its source took. A 3-row `$$` block holds a display-style sum or fraction. A one-row `$$…$$` falls back to the smaller inline style. Inline math is limited to one row, so a tall inline formula (a matrix in `$…$`) stays text.
- **Selecting reflows the row.** When a drag reaches a formula, the formula turns back into its source and the rest of its row moves right to grid positions.
- **False positives.** The delimiter rules and the "looks like math" check keep ordinary shell output as text, but something like `$x=$y` or `$a_b$` in a log will be drawn as math. There is no `config.math` toggle yet.
- **Markdown damage can't always be undone.** When an agent ignores the prompt and writes bare math, Claude Code has already removed the backslash from `\{`, `\\` and `\,`. Wezmux renders what is left, which can be subtly wrong (`\{x\}` shows as `x`).
- **Dependency weight.** Wezmux's own code is about 300 lines plus tests. RaTeX is about 40k lines in 9 crates, plus 580 KB of KaTeX fonts, adding 26 crates to the lock file. All are pure Rust and MIT/Apache. Release binary growth is not measured yet.
- **Fidelity.** RaTeX claims over 99.5% KaTeX syntax coverage and does not cover every LaTeX package. `\text{}` uses KaTeX's serif font, not the terminal's.
