/**
 * ljos seat hook for omp, at ~/.omp/agent/extensions/ljos.ts.
 * Written by `ljos onboard --harness omp`; {ljos} is filled in there.
 *
 * Shells to `ljos hook` with the snake_case JSON it parses:
 *   before_agent_start -> UserPromptSubmit: the seat's memories as a hidden custom message
 *   tool_call (bash)   -> PreToolUse: a seat rule's deny blocks, an ask asks the person
 *   session_shutdown   -> SessionEnd: the memories used in the session fire together
 *
 * The runner owns its conversation id. Ids inherited from a runner that
 * started this one (or from a line editor) are dropped at load, before omp
 * spawns MCP servers or shells, and one OMP_SESSION_ID is minted for this
 * process, so its claims are its own.
 *
 * Fails open: a missing binary, a timeout or bad JSON is no opinion. A throw
 * in a tool_call handler blocks the tool, so every path here catches.
 */
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";

const LJOS = process.env.LJOS_BIN ?? "{ljos}";
const TIMEOUT_MS = 4000;

for (const key of Object.keys(process.env)) {
	if ((key.endsWith("_SESSION_ID") || key.endsWith("_THREAD_ID")) && key !== "XDG_SESSION_ID") {
		delete process.env[key];
		delete Bun.env[key];
	}
}
const SESSION = crypto.randomUUID();
process.env.OMP_SESSION_ID = SESSION;
Bun.env.OMP_SESSION_ID = SESSION;

type HookOut = {
	hookSpecificOutput?: {
		additionalContext?: string;
		permissionDecision?: string;
		permissionDecisionReason?: string;
	};
};

async function hook(payload: Record<string, unknown>): Promise<HookOut | undefined> {
	try {
		const proc = Bun.spawn([LJOS, "hook"], {
			stdin: new TextEncoder().encode(JSON.stringify({ session_id: SESSION, ...payload })),
			stdout: "pipe",
			stderr: "ignore",
		});
		const timer = setTimeout(() => proc.kill(), TIMEOUT_MS);
		const text = await new Response(proc.stdout).text();
		clearTimeout(timer);
		if ((await proc.exited) !== 0 || !text.trim()) return undefined;
		return JSON.parse(text) as HookOut;
	} catch {
		return undefined;
	}
}

export default function ljosHook(pi: ExtensionAPI) {
	pi.on("before_agent_start", async event => {
		const out = await hook({ hook_event_name: "UserPromptSubmit", prompt: event.prompt });
		const context = out?.hookSpecificOutput?.additionalContext;
		if (!context) return;
		return { message: { customType: "ljos-memory", content: context, display: false } };
	});

	pi.on("tool_call", async (event, ctx) => {
		if (event.toolName !== "bash") return;
		const command = (event.input as { command?: unknown }).command;
		if (typeof command !== "string" || !command.trim()) return;
		const out = await hook({
			hook_event_name: "PreToolUse",
			tool_name: "Bash",
			tool_input: { command },
		});
		const spec = out?.hookSpecificOutput;
		const reason = spec?.permissionDecisionReason ?? "seat rule";
		if (spec?.permissionDecision === "deny") return { block: true, reason };
		if (spec?.permissionDecision === "ask") {
			// A print-mode run has nobody to ask.
			if (!ctx.hasUI) return { block: true, reason: `${reason}; needs a person to approve` };
			try {
				if (await ctx.ui.confirm("ljos seat rule", `${reason}\n\n${command}`)) return;
			} catch {
				// no dialog: fall through to block
			}
			return { block: true, reason };
		}
	});

	pi.on("session_shutdown", async () => {
		await hook({ hook_event_name: "SessionEnd" });
	});
}
