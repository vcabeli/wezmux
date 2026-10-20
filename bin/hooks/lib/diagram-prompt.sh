# wezmux: the one-line prompt that tells an agent Wezmux draws Mermaid between
# <wezmux_diagram> tags in place of the source (docs/inline-diagrams.md).
# Sourced by bin/claude (--append-system-prompt) and the Codex SessionStart
# hook (additionalContext). Set WEZMUX_DIAGRAMS=0 to leave it out.
WEZMUX_DIAGRAM_PROMPT='You are running inside Wezmux, which draws Mermaid diagrams inline. When a diagram would help, put Mermaid source between a line <wezmux_diagram> and a line </wezmux_diagram> instead of a code fence. It is drawn in the rows its source occupies, so keep it compact and prefer left-to-right layouts (flowchart LR).'
