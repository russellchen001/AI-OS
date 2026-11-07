/*
 * Conversation answers about "how do I do X on my computer" must match the
 * computer AI-OS runs on. Without this a local model defaults to Windows
 * steps (seen on the phone remote, 2026-11-01).
 */
export function deviceContextMessage(
  userAgent: string = typeof navigator === "undefined" ? "" : navigator.userAgent,
): { role: "system"; content: string } | undefined {
  if (!/Macintosh|Mac OS X/i.test(userAgent)) return undefined;
  return {
    role: "system",
    content: [
      "The user runs AI-OS on a Mac (macOS).",
      "This describes the current host only. It does not determine the operating system, hardware, budget or requirements of a new device the user may buy or deploy. Ask for missing target requirements instead of assuming they match this Mac.",
      "When explaining how to do something on their computer, give macOS steps (Finder, System Settings, ⌘ shortcuts), never Windows steps, unless the user asks about another system.",
    ].join("\n"),
  };
}
