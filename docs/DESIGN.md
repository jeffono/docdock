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
- Code syntax uses the GitHub Light Default and GitHub Dark Default colors (chosen by the user): red
  keywords and operators, dark-blue strings, blue constants, properties and definitions, violet
  functions, orange types, green tags and gray comments. Punctuation and other identifiers keep the
  foreground color. Shared by reading and editing, without added emphasis or italics.

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

## Note list

One list fills the panel under the search bar at the top. Every note is the same card component;
pinned and unpinned notes differ only in section and starting state.

- **Sections:** "Pinned" cards in `_pinned.md` order, then "Notes" as one-line rows. Typing filters
  both in place and pinned matches stay cards. The root `_pinned.md` file is hidden from the list and
  search. A "Create “…”" item appears when no visible note title matches exactly.
- **Order:** a quiet native select beside "Notes" and the same select in Preferences > General share
  one saved choice: name (display title), modified or created, ascending or descending. The default is
  modified, newest first. Only the unfiltered Notes section is sorted; searches keep relevance and
  Pinned keeps file order. Creation time comes from the filesystem, falling back to modified time.
- **Date groups:** an optional checkbox beside the order selector, also in Preferences > General,
  defaults off. It groups only unfiltered Notes when ordered by modified or created date. Name order
  keeps the choice but disables the checkbox with a hint. Quiet labels use the section-label style:
  Today, Yesterday, Previous 7 days (2–7 days ago), Previous 30 days (8–30 days ago), then earlier
  months of this year (month names in the user's locale), then earlier years. Boundaries use local
  calendar dates; ascending order puts older groups first. Labels are not interactive or selectable.
- **Reorder pins:** drag a pinned header onto another pinned item to move it to that position, or use
  Alt+Shift+↑ / Alt+Shift+↓ on the selected pinned item (including web and missing items). Selection
  follows the item. Reordering is unavailable while editing. A stale file produces a nearby error.
- **Web pins:** one-line cards in the Pinned section, in file order alongside notes: link label as
  title, host as secondary text, no chevron or preview. Click or Enter opens the page in the panel;
  the bookmark removes it. Existing web pins remain usable; new pages are saved as notes and pinned from their cards.
- **Page notes:** the shared bar offers a "+" action: create a note in Notes (typed text becomes its title, otherwise Untitled), or save the current page in Browser. It saves the page title,
  URL and quoted selection, then returns to notes with the new card expanded and editing, ready for
  annotations. A note with an http(s) `url:` has an "Open page" arrow action left of the pencil; it
  keeps the usual expand/edit behavior and header action visibility. Other URL schemes have no action.
- **States:** line (title, modified date and first tag, with +N for more tags); preview (up to about
  eight lines, fading at the bottom; the default for pinned cards); expanded (full content, reading);
  editing (expanded, with the editor). Only line versus open is remembered, per computer; expanded and
  editing return to preview when the panel closes.
- **Expanded:** no height limit and no inner scroll. While scrolling inside the card, its header
  (title and actions) sticks to the top. Only one card is expanded: expanding another returns
  the previous one to preview. Clicking a Notes row turns it into an expanded card in place, and it
  returns to a row when closed; there is no separate note view.
- **Editing:** The browser "+" opens a newly captured page in edit mode. Otherwise use the pencil in
  the card header or Ctrl+E on the selected card; entering from preview expands the card. One card edits at a time and shows its own save status, conflict and
  errors. If the search would hide it, it stays at the top until editing ends. Closing the card,
  expanding another, typing in the search or the panel closing first leave editing, which saves; on a
  conflict or error nothing else happens until it is resolved. The Note editor rules below apply
  inside the card.
- **Mouse:** the header and the chevron toggle line and open (preview for pinned cards, expanded for
  Notes rows); the preview body and its "Show all" button expand the card; an expanded pinned card ends
  with "Show less", back to preview. Links work in reading.
- **Alignment:** one content column for the whole panel. The top bar title, the search field, section
  labels and cards start `--space-4` + `--space-2` from the panel's left edge and end `--space-3` from
  the right edge; the left inset holds the chevron gutter. Nothing else uses its own margin.
- **Selection:** a line, like a missing-note row, keeps the selected background, accent title and
  accent border. An open card shows only the accent border, so long content and text selection stay
  readable. The editing card shows no selection style; its editing border already marks it.
- **Chevron:** in the gutter, aligned with the title, right for a line and down for an open card. It
  shows on hover, on the selected item and on keyboard focus; its space is always reserved.
- **Header actions:** no expand or collapse button. From right to left, pin, then the pencil only when
  expanded, with `--space-1` gaps and `--size-control` click targets. Both show only on hover, on the
  selected item and on keyboard focus, with their space reserved. Icons use `--size-icon`, muted color,
  no border and no background; the bookmark is filled when pinned and outline otherwise. While
  editing, the pencil stays visible in the accent color, still without border or background. Hover
  keeps the lighter surface and foreground color. Tooltips and accessible labels include shortcuts:
  "Pin (Ctrl+D)" / "Unpin (Ctrl+D)", "Edit (Ctrl+E)", "Open (→)" / "Close (←)" and "Remove from
  pinned (Delete)".
  Headers have `--space-1` inner padding on every side in every state; missing-note rows use the same
  vertical and right inset and a quiet remove icon.
- **Focus:** clicking the panel or the shortcut gives it focus; it then stays open until focus moves to
  another app, Esc from the empty search bar, or the shortcut from the search bar. A pinned panel only
  closes by unpinning. Without focus (hover only), it closes when the cursor leaves. While the panel or its
  page has focus, a 2 px `--color-accent` band runs the full height of the inner edge, over the border and
  above the content, without moving anything.
- **Tab stops:** search bar → recovery actions when shown ("Restore" on leftovers, then "Restore
  pinned items") → list → note order → date grouping (when available) → top bar buttons (including
  Preferences), reversed with Shift+Tab. Card
  controls stay clickable and labeled but are outside the Tab sequence. The list is one Tab stop;
  entering it without a selection selects the first item. Its only focus indicator is the selected
  card's existing style, without an additional outline. The editor keeps Tab indentation. The list is a
  labeled region of plain groups, not a listbox, since cards hold buttons, links and the editor; the
  selected item has `aria-current` and a polite live region announces it.
- **Keyboard:** with focus anywhere in the panel except a text field or the editor, the list keys below
  work as from the search bar, and typing a character goes to the search bar. Enter acts on the
  selection even on a focused button; Space presses the button.
- **Details:** the "Create" item is the first item, under the bar; the first match is selected, so Enter
  expands it and Ctrl+Enter or ↑ reaches Create. Pinning uses a bookmark icon, distinct from the header's
  thumbtack that keeps the panel open. Rows and lines show a snippet with the matches highlighted under
  the title; previews and expanded cards show the content. Reading cards hide the frontmatter title
  (the header has it); the frontmatter title stays visible while editing. The expanded card lists every
  tag. → and ← act on the selected card only with the caret at the end of the text, otherwise they move it.

| Shortcut                  | Action                                                                                   |
| ------------------------- | ---------------------------------------------------------------------------------------- |
| ↑ / ↓                     | Select a card or row; on a long expanded card, first scroll about three lines to its end |
| Alt+Shift+↑ / Alt+Shift+↓ | Move the selected pinned item up / down                                                  |
| Alt+↑ / Alt+↓             | Select the previous / next item directly                                                 |
| PgUp / PgDn               | Scroll the list by about one screen                                                      |
| → / ←                     | Open / close one level: line, preview, expanded                                          |
| Enter                     | Expand the selected item, run "Create", or remove a selected missing pinned item         |
| Delete                    | Remove the selected missing pinned item                                                  |
| Ctrl+Enter                | Create a note with the exact typed text                                                  |
| Ctrl+D                    | Toggle pin on the selected card from search, the list or its editor                      |
| Ctrl+E                    | Toggle editing on the selected card                                                      |
| Ctrl+F                    | Focus the search bar and select its text                                                 |
| Esc                       | Leave editing; next, clear the search and return to the bar; from the empty bar, close   |
| Ctrl+Alt+Space            | Open and go to the search bar with its text selected; from the search bar, close         |

## Preferences

The top bar gear replaces the folder action and opens a third view with quiet General, Appearance, Browser
and Shortcuts section buttons. The page stays loaded but hidden. The gear or Esc returns to the previous
Notes/Browser tab; either tab also leaves Preferences. The global shortcut returns to Notes and search.
General contains the folder path, folder picker, note order, date grouping and panel side; side changes
mirror the tray. Browser
shows DuckDuckGo as read-only. Shortcuts lists panel/list keys and derives formatting keys from the
editor bindings. Appearance shows the panel width, how to resize it and a reset. Sections scroll
inside the panel, use the same insets and tokens, and have no new colors. Opening a page clears the
Notes query so returning to Notes starts with an empty search.

## Panel navigation

The top bar shows subtle Notes and Browser tabs, followed in Browser by discreet back/forward
controls, then the Preferences gear and panel pin. The panel is 460 logical pixels wide by default;
a 6-pixel handle along the inner edge resizes it (double-click restores the default).
The panel pin is filled when active, outlined otherwise, and 12% smaller than other icons.
Ctrl+Tab switches Notes/Browser while the main window has focus, including in the editor.
Button tooltips include their working shortcuts. There is no app name or page title. The same search
field and "+" action occupy the second bar in both modes. Browser starts with a short hint and the
field focused; switching tabs retains the page document and history. The native page is clipped two
logical pixels inside the panel sides and bottom so the focus band remains visible.

## Unified address bar

In browser mode the bar shows the current page URL; typing offers an Open/Search action, matching
local addresses, then engine suggestions. The same arrows, Enter and selected row style apply.
When the suggestion list is closed, ↓ opens the five most recent local addresses (including
search result URLs), even with an empty bar or the current URL. This does not request engine
suggestions; typing resumes the normal suggestion rules.
Suggestions float below the bar on one solid surface, with icons for search, history and Open;
only the selected row is highlighted. The native page is clipped behind them without moving or resizing. Esc restores
the current URL and dismisses suggestions before the next Esc closes the panel.

In notes mode Enter always activates the selected entry. URL-like text offers an explicit Open entry
after notes; only an explicit http(s) prefix selects it by default. Otherwise matches, then Create,
keep priority, including names such as `todo.md` and `1.5`. Other text offers Search the web; choosing
it enters `?` mode and enables remote suggestions, as does typing `?`. Ordinary note queries remain
local. Web actions use neutral icon rows; all untrusted suggestions appear as text.

## Note editor

The editor lives in the card being edited, in place of its content; there is no separate note view and
no floating chrome. The card keeps its header and its button order: the pencil, left of pin, stays
visible in the accent color while editing (see Header actions), and the small Saving… / Saved status sits in the
header, left of the actions. The status is absent on conflict or error, whose notices sit at the top
of the card body, above the text. The editing card has an accent border, and its text keeps the reading body's padding, so
toggling does not move it. Reading remains the default. Light-theme tags use the selected background
and accent text in both views; dark-theme tags keep their surface background and muted text.
The feature-specific Editor uses Segoe UI for text and Cascadia Mono for code, with existing tokens.
Markdown emphasis uses italics to represent the content. Syntax is visible on selection lines and
hidden elsewhere. Conflict actions appear above the editor: one primary Keep my version button and
a secondary Discard my changes button. Errors appear in red beside this notice.

Windows editor shortcuts:

| Shortcut                                      | Action                                                                                     |
| --------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Ctrl+B / Ctrl+I / Ctrl+Shift+X / Ctrl+Shift+` | Toggle bold / italic / strikethrough / inline code                                         |
| Ctrl+J                                        | Join the current and next line, or all selected lines; strip following list/quote prefixes |
| Ctrl+K                                        | Insert a link; put the cursor in its destination, or its label when empty                  |
| Ctrl+1…6 / Ctrl+0                             | Toggle heading level / make paragraph                                                      |
| Ctrl+Shift+8 / Ctrl+Shift+7 / Ctrl+Shift+9    | Toggle bullet / numbered / task list                                                       |
| Ctrl+Shift+.                                  | Toggle blockquote                                                                          |
| Enter / Backspace                             | Continue / remove Markdown list or quote markup                                            |
| Tab / Shift+Tab                               | Indent / outdent list items                                                                |
| Ctrl+E                                        | Toggle reading and editing from anywhere in the panel                                      |

Inline formatting wraps the selection, unwraps matching markers inside or included in the selection,
and inserts a pair around an empty cursor. Block commands touch only selected line prefixes. Every
formatting command is one undo step. Shared checkbox styling and code-block framing keep the same
appearance in reading and editing.
