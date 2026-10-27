# wezmux: the prompt that tells an agent what Wezmux draws in place of its
# source: Mermaid between <wezmux_diagram> tags (docs/inline-diagrams.md) and
# LaTeX math between $ and $$ (docs/inline-math.md).
# Sourced by bin/claude (--append-system-prompt) and the Codex SessionStart
# hook (additionalContext). Set WEZMUX_DIAGRAMS=0 or WEZMUX_MATH=0 to leave
# either part out; WEZMUX_INLINE_PROMPT is empty when both are.
WEZMUX_DIAGRAM_PROMPT='You are running inside Wezmux, which draws Mermaid diagrams inline. When a diagram would help, put Mermaid source between a line <wezmux_diagram> and a line </wezmux_diagram> instead of a code fence. It is drawn in the rows its source occupies, so keep it compact and prefer left-to-right layouts (flowchart LR).'
WEZMUX_MATH_PROMPT='Write inline math as $...$ inside backticks, like `$E = mc^2$`, and display math as a ```latex code block holding a line $$, the formula, and a line $$. Without the backticks or the code block, the terminal'"'"'s Markdown rendering strips backslashes and underscores. A formula is drawn in the rows its source occupies, so put each row of a matrix or aligned environment on its own line.'

WEZMUX_INLINE_PROMPT=
if [ "${WEZMUX_DIAGRAMS:-1}" = 1 ]; then
    WEZMUX_INLINE_PROMPT="$WEZMUX_DIAGRAM_PROMPT"
fi
if [ "${WEZMUX_MATH:-1}" = 1 ]; then
    if [ -n "$WEZMUX_INLINE_PROMPT" ]; then
        WEZMUX_INLINE_PROMPT="$WEZMUX_INLINE_PROMPT It also draws LaTeX math inline. $WEZMUX_MATH_PROMPT"
    else
        WEZMUX_INLINE_PROMPT="You are running inside Wezmux, which draws LaTeX math inline. $WEZMUX_MATH_PROMPT"
    fi
fi
