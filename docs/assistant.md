# GeenioAI, the AI assistant

> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-10-18 · **Change:** first version (the in-app assistant, gaps #25) · **Target:** Microsoft Word (Microsoft 365) Copilot pane and margin icon

WordCraft's answer to Word's Copilot: a chat pane and a margin icon whose agent writes and edits
the open document with WordCraft's own commands. Any model can sit behind it: the desktop app and
the CLI reach models through a [FANTASTICO](https://fantastico.cc) account (one prepaid credit,
every major LLM: Claude, GPT, Gemini, Llama…), and the model is chosen in the pane's settings.

## What the user sees

| Where | What | Word |
|---|---|---|
| Home › GeenioAI | Opens and closes the pane (`ui.assistant`) | Home › Copilot |
| The pane (right) | Chat with the assistant; suggestions until the conversation starts (Summarize this document, Suggest a title, Check the tone); a status line in plain words while it works ("Writing…", "Running Bold…"); a confirmation card when it wants to start a new document, open or save a file; the credit, a fresh start (behind a confirmation) and the settings in the header | Copilot pane |
| The margin icon | Beside the paragraph the caret is in (the selection's first line when there is one), in the left margin; with that line scrolled out of view it waits at the top or bottom edge of the page area, so it is always there to click. A click opens a prompt box ("Describe what you'd like to write" on an empty paragraph, Word's Draft with Copilot) and the quick actions that fit: Rewrite the selection, Make it shorter, Fix spelling and grammar, Summarize the selection; Rewrite this paragraph; Open GeenioAI | Copilot icon in the margin |
| Settings (gear) | Reply language (automatic, the interface language, English); the model, from the account's list with a search box; the account and its credit; change the account key | — |

Every request goes with where the user is in the document: the selected text (which `type_text`
replaces), an empty paragraph the caret is on, or the paragraph around the caret. The assistant
rewrites in place, drafts at the caret, and answers questions in the chat. **One turn is one undo
step**: Undo takes back everything a request changed.

## Keys and privacy

- The onboarding asks for a FANTASTICO **account key** (`ck-fant-…`). The desktop app checks it
  with the account service before storing it in `<settings>/assistant/fantastico.key` (mode 0600
  on Unix). Nothing in the interface can read the key back: the pane can store one or replace it
  (Settings › Change account key), never show it. No command, control method or log line carries it.
- Prompts and the document text go straight to the model provider through FANTASTICO's edge
  endpoint; FANTASTICO mints short-lived, capped runtime keys and never sees the prompts.
- Runs without preferences (`WORDCRAFT_NO_PREFS`, agents' test runs) have no assistant and never
  touch the key. The web build says the assistant isn't available yet (no host).

## For agents and scripts

- `ui.assistant {"value"?: bool}` shows or hides the pane (toggles without `value`); the result
  says whether a host is attached (`available`) and a key is stored (`connected`).
- `ui.assistant.ask {"prompt": "…", "draft"?: bool}` opens the pane and sends the prompt with the
  document context; `draft` asks for the answer in the document, as the margin icon's box does.
- `wordcraft-cli ask "PROMPT" [--file F | --template T] [--model M] [--max-steps N] [--yes]
  [--author NAME] [--save OUT]` runs the same agent headlessly on a document, with the key in
  `$FANTASTICO_KEY` (an account key, or a project key `sk-fant-…` for a trusted backend). The reply
  goes to stdout, progress to stderr; `--yes` allows new/open/save without asking.

## How it works

| Layer | Crate | Job |
|---|---|---|
| L5 | `wordcraft-assistant` | The agent loop: the model asks for tools, they run against any `wordcraft_mcp::Backend` (a headless session or the app), their results go back until the model answers. No network code: the host brings the model as a `Provider` (OpenAI-style chat completions). Model output is hostile input: at most 20 steps a turn and 16 tool calls a reply, cut results, malformed arguments and unknown tools answered with errors, old turns forgotten whole. Out of steps, the model gets one last call without tools to say what it did |
| L6 | `wordcraft-ui-egui::assistant` | The pane and the margin icon. The loop runs on a worker thread; its tools reach the document through the control methods, run on the UI thread each frame (`tick`), joined into one undo step per turn |
| app | `apps/wordcraft/src/assistant_host.rs` | The FANTASTICO host: the key store and the `fantastico-sdk` client (models, balance, chat completions) |

The tools offered to the model are the command-level MCP tools: `get_text`, `inspect_document`,
`select_text`, `type_text`, `execute`, `batch`, `list_commands`, and, behind a confirmation,
`new_document`, `open_document`, `save_document`. Pictures and pointer tools stay out.

## Known gaps

- No streaming: the reply shows when the turn ends (the status line shows what it is doing).
- No tracked changes for the assistant's edits (Word's Copilot rewrites offer a Keep/Discard
  preview); Undo takes a turn back in one go.
- No "Visualize as a table" or other Copilot-specific actions beyond the ones above.
- The margin icon is drawn only for body text (not in headers, footers, notes, text boxes or
  tables with whole cells selected).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-18 | major | First version: pane, margin icon, FANTASTICO host, `wordcraft-cli ask` |
