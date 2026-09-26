// ljos seat hook for opencode, at ~/.config/opencode/plugins/ljos.ts.
// Written by `ljos onboard --harness opencode`; {ljos} is filled in there.
//
// The seat's memories on each user prompt, argv law on each bash call, and
// the session id in every shell the agent opens. A hook that fails has no
// opinion: a missing binary, a timeout or bad JSON lets the work go on.
import type { Plugin } from "@opencode-ai/plugin"

const LJOS = process.env.LJOS_BIN ?? "{ljos}"
const TIMEOUT_MS = 20_000

// The context the last prompt raised, per session; the system transform
// hands it to every model call of that turn.
const held = new Map<string, string>()

async function hook(payload: Record<string, unknown>): Promise<any | undefined> {
  const proc = Bun.spawn([LJOS, "hook"], {
    stdin: new Blob([JSON.stringify(payload)]),
    stdout: "pipe",
    stderr: "ignore",
  })
  const timer = setTimeout(() => proc.kill(), TIMEOUT_MS)
  try {
    const text = (await new Response(proc.stdout).text()).trim()
    await proc.exited
    if (!text) return undefined
    return JSON.parse(text).hookSpecificOutput
  } catch {
    return undefined
  } finally {
    clearTimeout(timer)
  }
}

export const LjosSeat: Plugin = async () => ({
  // A shell the agent opens carries its session, so `ljos` there holds
  // under the conversation rather than under the shell's process.
  "shell.env": async (input, output) => {
    if (input.sessionID) output.env.OPENCODE_SESSION_ID = input.sessionID
  },

  "chat.message": async (input, output) => {
    const prompt = output.parts
      .filter((p: any) => p.type === "text" && !p.synthetic)
      .map((p: any) => p.text)
      .join("\n")
      .trim()
    if (!prompt) return
    const out = await hook({
      session_id: input.sessionID,
      hook_event_name: "UserPromptSubmit",
      prompt,
    })
    const ctx = out?.additionalContext
    if (ctx) held.set(input.sessionID, ctx)
    else held.delete(input.sessionID)
  },

  "experimental.chat.system.transform": async (input, output) => {
    const ctx = input.sessionID ? held.get(input.sessionID) : undefined
    if (ctx) output.system.push(`<ljos-seat-memory>\n${ctx}\n</ljos-seat-memory>`)
  },

  // A throw here reaches the model as the tool's error and the command
  // does not run. opencode cannot raise its approval prompt from a plugin,
  // so an ask stops the command and tells the agent to ask the person.
  "tool.execute.before": async (input, output) => {
    if (input.tool !== "bash" || typeof output.args?.command !== "string") return
    const out = await hook({
      session_id: input.sessionID,
      hook_event_name: "PreToolUse",
      tool_name: "Bash",
      tool_input: { command: output.args.command },
    })
    const verdict = out?.permissionDecision
    if (verdict === "deny") throw new Error(`ljos: denied: ${out.permissionDecisionReason}`)
    if (verdict === "ask")
      throw new Error(
        `ljos: needs the person's approval: ${out.permissionDecisionReason}. ` +
          `Ask them; do not retry until they say yes.`,
      )
  },

  event: async ({ event }) => {
    if (event.type === "session.deleted") {
      const id = (event as any).properties?.info?.id
      held.delete(id)
      await hook({ session_id: id, hook_event_name: "SessionEnd" })
    }
  },
})
