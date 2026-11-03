# Design

Identity chosen by the user: a neutral, compact system utility. Values live in
`src/lib/styles/tokens.css`; this file says how to use them. Only the user changes the identity.

## Principles

- **Quick reference:** the panel is narrow and opens briefly. Content first, with minimal controls.
- **Density:** more content per screen, without crowding the text.
- **Subtle:** color is a signal, not decoration.

## Color

- Neutrals for almost everything. Blue accent marks only selection, enabled state, links, focus and the primary action.
- Light and dark themes follow Windows. Every new color starts in both themes, in `light-dark`.
- Red only for errors. Fixed colors outside `tokens.css` fail `check:fast`.

## Typography

- Segoe UI throughout, and Cascadia Mono only for code.
- Body in `--font-size-md`. Lists, fields and tables in `--font-size-sm`.
- Weights 400 and 600, no decorative italics.

## Space and shape

- Spacing only from the `--space-*` scale, in 4 px increments.
- Controls use `--size-control`, bars use `--size-bar` and corners use `--radius-sm`.
- `--color-line` lines separate areas; no shadows.

## Components

Use `src/lib/ui/` before creating anything. Create a component only if nothing existing works.

| Component    | Use                                                  |
| ------------ | ---------------------------------------------------- |
| `Button`     | Primary action, at most one per area                 |
| `IconButton` | Secondary action, always labeled; toggles on and off |
| `TextField`  | Single-line input; Esc leaves the field              |
| `Icon`       | Lucide stroke, text color, width 2                   |

## States

- **Hover:** lighter surface, without shifting anything.
- **Focus:** 2 px accent outline, keyboard only.
- **Selected or enabled:** `--color-selected` background with accent text.
- **Empty:** one sentence explaining what to do and the action immediately below.
- **Error:** in red, near the cause, saying what happened.

## Motion

- Panel slides in 150 ms, defined in `dock.rs`. Hover uses `--duration`.
- Nothing animates for decoration or delays access.

## Text

- English by default and pt-BR, both in `src/lib/i18n.ts`; tray labels in `tray.rs`. Short and direct
  sentences.
- Buttons with infinitive verbs: "Open", "Choose folder".
- No exclamation marks, technical jargon or blaming the user.

## Rendered content

Reading and editing share `features/notes/content.css`, scoped by `.note-content`, including the
frontmatter title and tag header. The editor loads it through an external link in its shadow root.
CSP forbids inline styles: no component depends on a `style` attribute. Footnote references and
labels are small raised text, references in the accent color; definitions are small muted text where
they are written. Raw HTML keeps its shape as plain text. List paragraphs stay tight, like editor lines.

GitHub alert titles use the English kind names, matching the Markdown source, without icons. Note,
Tip and Important reuse the accent token; Warning and Caution reuse the danger token as content
semantics. No colors were added. Raw frontmatter uses the existing surface, line and monospace tokens.

## Note editor

The fully opaque pencil IconButton floats at the note area's top-right corner in both modes, always
visible and keyboard reachable with the standard focus ring. It stays pressed during editing. Only the
first block reserves room for it and the small Saving… / Saved status, in both modes; the rest of the
note uses the full width, and the button scrolls away with the content (Ctrl+E still works). The
status is absent on conflict or error, whose notices already explain the problem. The note area keeps
a 1 px transparent border in reading and an accent border in editing, with `--radius-sm`, so toggling
does not move the frame. Reading remains the default. Light-theme tags use the selected background
and accent text in both views; dark-theme tags keep their surface background and muted text.
The feature-specific Editor uses Segoe UI for text and Cascadia Mono for code, with existing tokens.
Markdown emphasis uses italics to represent the content. Syntax is visible on selection lines and
hidden elsewhere. Conflict actions appear above the editor: one primary Keep my version button and
a secondary Discard my changes button. Errors appear in red beside this notice.

Windows editor shortcuts:

| Shortcut                                      | Action                                                                    |
| --------------------------------------------- | ------------------------------------------------------------------------- |
| Ctrl+B / Ctrl+I / Ctrl+Shift+X / Ctrl+Shift+` | Toggle bold / italic / strikethrough / inline code                        |
| Ctrl+K                                        | Insert a link; put the cursor in its destination, or its label when empty |
| Ctrl+1…6 / Ctrl+0                             | Toggle heading level / make paragraph                                     |
| Ctrl+Shift+8 / Ctrl+Shift+7 / Ctrl+Shift+9    | Toggle bullet / numbered / task list                                      |
| Ctrl+Shift+.                                  | Toggle blockquote                                                         |
| Enter / Backspace                             | Continue / remove Markdown list or quote markup                           |
| Tab / Shift+Tab                               | Indent / outdent list items                                               |
| Ctrl+E                                        | Toggle reading and editing from anywhere in the panel                     |

Inline formatting wraps the selection, unwraps matching markers inside or included in the selection,
and inserts a pair around an empty cursor. Block commands touch only selected line prefixes. Every
formatting command is one undo step. Shared checkbox styling and code-block framing keep the same
appearance in reading and editing.
