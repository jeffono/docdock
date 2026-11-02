# DocDock — agent instructions

Autohide sidebar for Windows: touching the screen edge with the mouse expands it over other windows to
browse, add and edit references (markdown, images, web pages).

Single source of instructions for Claude, Codex and Antigravity. Codex and Antigravity read this file
directly, and `CLAUDE.md` only imports it. Do not create `GEMINI.md`: Antigravity does not expand `@`
imports. This file is loaded every session: keep it short.

## Where to start

`docs/PLAN.md` says what to do now. Consult `docs/ARCHITECTURE.md` and `docs/decisions/` only when the
task requires it. Do not reopen recorded decisions without new evidence.

## Commands

| Command           | Purpose                                     |
| ----------------- | ------------------------------------------- |
| `pnpm install`    | Install dependencies and activate hooks     |
| `pnpm tauri dev`  | Run the app with UI hot reload              |
| `pnpm format`     | Format TypeScript, Svelte and Rust          |
| `pnpm check:fast` | Lint, types and formatting; use during work |
| `pnpm check`      | Full verification; use on task completion   |
| `pnpm test`       | Frontend tests                              |

Use only pnpm. Hooks run `check:fast` on commit and `check` on push: never use `--no-verify`.

## Simplicity

Solve today's problem with the simplest robust solution. No abstraction before the second real use,
no speculative configuration. Before adding, see if something can be removed. A new dependency needs
a clear reason in the commit message.

## Architecture

- The UI talks to Rust only through `src/lib/ipc.ts`.
- Win32 and other system APIs belong in `src-tauri/src/platform/`.
- Separate decisions from effects: rules are pure functions; system calls are a thin layer.
- User content is ordinary `.md` files. Everything derived must be reconstructible.
- Never lose content: write atomically and do not overwrite external changes without warning.
- Visuals: use tokens and `src/lib/ui/`, never fixed values. Only the user changes the identity; create a
  token or component only if nothing existing works. For UI, read `docs/DESIGN.md` and deliver a
  screenshot for approval. Prototypes are excluded.
- Rendered content (markdown, HTML, web pages) is untrusted.

## Tests

Test what is costly when it breaks: content loss, state machines, file paths and security. Do not test
trivial forwarding, appearance or the framework. Before adding a test, answer what real bug it would
catch. Bug fixes get a regression test if the bug can recur. If the suite exceeds 30 tests or 30 seconds,
review it. Prefer running only the relevant test during work.

## Definition of done

- `pnpm check` passes.
- Visual behavior (window, focus, animation) has a manual verification script in the delivery.
- Anything that could not be verified is stated in the delivery.
- Affected documents are updated.

## Efficient quota use

Each plan task has a level; use the cheapest model that can resolve it well.

| Level      | Use                                                          |
| ---------- | ------------------------------------------------------------ |
| `basic`    | Mechanical changes, renames, text, well-specified tasks      |
| `standard` | Functionality with an established pattern                    |
| `advanced` | Architecture, Win32, focus, concurrency, security, hard bugs |

- Read only what is necessary. Delegate only when worthwhile: a subagent starts without context.
- After two unsuccessful attempts, stop and move up a level instead of persisting.

## Context and documentation

All durable context stays in the repository. Private agent memory is not shared.

| Information                             | Location               |
| --------------------------------------- | ---------------------- |
| Current state, next step, open task     | `docs/PLAN.md`         |
| How the system works today              | `docs/ARCHITECTURE.md` |
| Why a costly-to-reverse decision exists | `docs/decisions/`      |
| Details of a change                     | Commit message         |

- When pausing a task, leave a line in "In progress" in the plan; remove it when done.
- Each piece of information lives in one place. Updating replaces the old text.
- A completed stage becomes one line. No changelog, journal or README per folder.

## Work between agents

- One task per branch, with a short descriptive English name, such as `atomic-save`. In parallel, use one
  worktree per agent in `../docdock-wt/<branch>`.
- Check `git status` and `git log -10` before starting and do not touch others' changes.
- Merge into `main` only when the user asks.
- Review by an agent from another provider, diff only: when closing each stage and before changes to
  persistence, security or plugins. Prototypes are excluded.
- When closing a stage, trigger CI manually if there is a remote.
- `pnpm tauri dev` uses port 1420 and the app occupies the screen edge: one instance per machine.

## Decisions

Record in `docs/decisions/` only what is costly to reverse: data format, security, plugins or a central
library. Use the next number and the sections Context, Decision and Consequences.

Decide reversible technical matters yourself. Ask the user only about product behavior, appearance without
a defined reference, costs and actions that are hard to undo.

## Language

Everything in the repository is in English: code, comments, commits, PRs, branches and documentation.
Replies to the user are in Brazilian Portuguese. UI text goes through internationalization, with English
as the default and pt-BR.
