# DocDock — agent instructions

Autohide sidebar for Windows: touching the screen edge with the mouse expands it over other windows to
browse, add and edit references (markdown, images, web pages).

Single source of instructions for all agents: `CLAUDE.md` only imports it, and no `GEMINI.md` exists
because Antigravity does not expand `@` imports. Loaded every session: keep it short.

## Commands

| Command           | Purpose                                     |
| ----------------- | ------------------------------------------- |
| `pnpm install`    | Install dependencies and activate hooks     |
| `pnpm tauri dev`  | Run the app with UI hot reload              |
| `pnpm format`     | Format TypeScript, Svelte and Rust          |
| `pnpm check:fast` | Lint, types and formatting; use during work |
| `pnpm check`      | Full verification; use on task completion   |
| `pnpm test`       | Frontend tests                              |
| `pnpm clean`      | Delete `.tmp/` and list merged leftovers    |
| `pnpm icons`      | Rebuild app icons from the original SVGs    |

Use only pnpm. Hooks run `check:fast` on commit and `check` on push: never use `--no-verify`.
`pnpm tauri dev` uses port 1420 and the app occupies the screen edge: one instance per machine.

## Documentation

All durable context stays in the repository; private agent memory is not shared. Read only what the
task needs: start with the plan.

| Information                                         | Location               |
| --------------------------------------------------- | ---------------------- |
| Next step and open task                             | `docs/PLAN.md`         |
| How the system works: modules and invariants        | `docs/ARCHITECTURE.md` |
| Visual identity and visible behavior                | `docs/DESIGN.md`       |
| Why a costly-to-reverse decision exists             | `docs/decisions/`      |
| Directions studied but not adopted                  | `docs/EXPLORATIONS.md` |
| Implementation details                              | Comments in the code   |
| Installation and use, for users (English and pt-BR) | `README*.md`           |
| History and details of a change                     | Git and commit message |

- Each piece of information lives in one place; elsewhere, link to it. Updating replaces the old text.
- When pausing a task, leave a line in "In progress" in the plan; remove it when done. A completed
  stage leaves the plan. No changelog, journal or README per folder.
- Record in `docs/decisions/` only what is costly to reverse: data format, security, plugins or a
  central library. Use the next number and the sections Context, Decision and Consequences. Do not
  reopen a recorded decision without new evidence.
- Decide reversible technical matters yourself. Ask the user only about product behavior, appearance
  without a defined reference, costs and actions that are hard to undo.

## Rules

- Simplicity: the simplest robust solution to today's problem. No abstraction before the second real
  use, no speculative configuration; before adding, see what can be removed. A new dependency needs a
  clear reason in the commit message.
- The UI talks to Rust only through `src/lib/ipc.ts`. Win32 and other system APIs belong in
  `src-tauri/src/platform/`.
- Separate decisions from effects: rules are pure functions; system calls are a thin layer.
- User content is ordinary `.md` files. Everything derived must be reconstructible.
- Never lose content: write atomically and do not overwrite external changes without warning.
- Rendered content (markdown, HTML, web pages) is untrusted.
- Visuals: use tokens and `src/lib/ui/`, never fixed values. Only the user changes the identity; create
  a token or component only if nothing existing works. For UI, read the identity part of
  `docs/DESIGN.md` and the section of the feature touched, and deliver a screenshot for approval.
  Prototypes are excluded.

## Tests

Test what is costly when it breaks: content loss, state machines, file paths and security. Do not test
trivial forwarding, appearance or the framework. Before adding a test, answer what real bug it would
catch. One behavior per test, so a failure names what broke. Bug fixes get a regression test if the bug
can recur. Prefer running only the relevant test during work.

- Time budget, excluding compilation: `pnpm test` under 10 seconds, `cargo test` under 30 seconds.
- When closing a stage, review the tests: remove those for removed code or already covered elsewhere,
  and split those that mix unrelated cases. Justify growth above 30% in the closing commit message.

## Definition of done

- `pnpm check` passes.
- Visual behavior (window, focus, animation) has a manual verification script in the delivery.
- Anything that could not be verified is stated in the delivery.
- Affected documents are updated.

## Models and delegation

Each plan task has a level; use the cheapest model that can resolve it well.

| Level      | Use                                                          |
| ---------- | ------------------------------------------------------------ |
| `basic`    | Mechanical changes, renames, text, well-specified tasks      |
| `standard` | Functionality with an established pattern                    |
| `advanced` | Architecture, Win32, focus, concurrency, security, hard bugs |

- A subagent starts without context: delegate only a task that can be briefed in a few lines. Then, to
  spread usage limits, send `basic` and `standard` tasks to OpenAI models (Codex) and `advanced` ones
  to Claude Opus. Antigravity is also an option, mainly its Neon and Hydrogen models.
- After two unsuccessful attempts, stop and move up a level instead of persisting.

## Git and work between agents

- One task per branch, with a short descriptive name, such as `atomic-save`. In parallel, use one
  worktree per agent: T3 creates them in `~/.t3/worktrees/docdock/`; otherwise use
  `../docdock-wt/<branch>`.
- Check `git status` and `git log -10` before starting and do not touch others' changes.
- Merge into `main` only when the user asks. Whoever merges removes the branch's worktree and its local
  and remote branch.
- Review by an agent from another provider, diff only: when closing each stage and before changes to
  persistence, security or plugins. Prototypes are excluded.
- When closing a stage, trigger the CI workflow manually; it does not run on push.

## Temporary files

- Disposable work files (scripts, browser profiles, screenshots, drafts) go in `.tmp/<branch>/` at the
  checkout root, never elsewhere; only tests that delete their own files may use the system temp folder.
- Evidence for approval is deleted once approved or when its branch is merged; it never enters the
  repository. `pnpm clean` deletes `.tmp/` and lists merged worktrees and branches to remove.

## Language

The repository is in English (code, comments, commits, PRs, branches and documentation), except the
pt-BR translations: `README.pt-BR.md` and UI text, which goes through internationalization with English
as the default. Replies to the user are in Brazilian Portuguese.
