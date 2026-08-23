import { closeSync, openSync, writeSync } from "node:fs";

const ttyPath = process.env.WEZMUX_TTY || "/dev/tty";
let tty;

function writeOsc(payload) {
  try {
    tty ??= openSync(ttyPath, "w");
    writeSync(tty, payload);
  } catch {
    // Status reporting must never interrupt the agent.
  }
}

function clean(value) {
  return String(value)
    .replace(/[\x00-\x1f\x7f;]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function emit(event, value) {
  const data = value === undefined ? "" : `;${clean(value)}`;
  writeOsc(`\x1b]7777;${event}${data}\x07`);
}

function notify(message) {
  writeOsc(`\x1b]9;${clean(message)}\x07`);
}

function messageText(message) {
  const content = message?.content;
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .filter((part) => part?.type === "text" && typeof part.text === "string")
    .map((part) => part.text)
    .join(" ");
}

function finalMessage(messages) {
  if (!Array.isArray(messages)) return "";
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i]?.role === "assistant") {
      const text = clean(messageText(messages[i]));
      if (text) return Array.from(text).slice(0, 200).join("");
    }
  }
  return "";
}

export default function wezmux(pi) {
  if (!process.env.WEZMUX) return;

  pi.on("session_start", () => emit("status", "idle"));

  pi.on("input", () => {
    emit("status", "working");
    notify("Oh My Pi is working...");
  });

  pi.on("agent_start", () => emit("status", "working"));

  pi.on("tool_call", (event) => {
    emit("tool", event.toolName);
    if (event.toolName === "ask") {
      emit("status", "needs_input");
      notify("Oh My Pi is waiting for your input");
    }
  });

  pi.on("tool_approval_requested", () => {
    emit("status", "needs_input");
    notify("Oh My Pi is waiting for approval");
  });

  pi.on("tool_approval_resolved", () => emit("status", "working"));

  pi.on("agent_end", (event) => {
    if (event.willContinue) return;
    const preview = finalMessage(event.messages);
    emit("status", "idle");
    if (preview) emit("message", preview);
    notify("Oh My Pi finished");
  });

  pi.on("session_shutdown", () => {
    if (tty === undefined) return;
    try {
      closeSync(tty);
    } catch {
      // The terminal may already be gone during process shutdown.
    }
    tty = undefined;
  });
}
