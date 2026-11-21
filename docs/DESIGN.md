# Design

The first part is the identity, chosen by the user: read it for any UI work. The rest specifies visible
behavior per feature: read only the section touched. Values live in `src/lib/styles/tokens.css`; how
they are implemented is in `docs/ARCHITECTURE.md` and the code.

# Identity

A neutral, compact system utility. Only the user changes the identity.

## Principles

- **Quick reference:** the panel is narrow and opens briefly. Content first, with minimal controls.
- **Density:** more content per screen, without crowding the text.
- **Subtle:** color is a signal, not decoration.

## Color

- Neutrals for almost everything. Blue accent marks only selection, enabled state, links, focus and the
  primary action.
- Light and dark themes follow Windows by default, with a System, Light or Dark choice in Preferences.
  Every new color starts in both themes, in `light-dark`. Fixed colors outside `tokens.css` fail
  `check:fast`.
- Red only for errors. Amber (`--color-warning*`) only tints notices that need attention without being
  errors, such as Undo after a delete. A light scrim (`--color-scrim`) dims the panel only behind a
  dialog.
- Code syntax uses the GitHub Light Default and GitHub Dark Default colors: red keywords and
  operators, dark-blue strings, blue constants, properties and definitions, violet functions, orange
  types, green tags and gray comments; punctuation and other identifiers keep the foreground color.
  Same in reading and editing, without added emphasis or italics.

## Typography

- Segoe UI throughout, and Cascadia Mono only for code.
- Body in `--font-size-md`. Lists, fields and tables in `--font-size-sm`.
- Weights 400 and 600, no decorative italics; Markdown emphasis uses italics as content.

## Space and shape

- Spacing only from the `--space-*` scale, in 4 px increments.
- Controls use `--size-control`, bars use `--size-bar`, icons `--size-icon` and corners `--radius-sm`.
- `--color-line` lines separate areas; no shadows, except a hovered floating window's (see
  "Floating images").
- One content column for the whole panel: bars, section labels and cards start `--space-4` +
  `--space-2` from the panel's left edge (the chevron gutter) and end `--space-3` from the right edge.
  Nothing else uses its own margin.

## Components

Use `src/lib/ui/` before creating anything. Create a component only if nothing existing works.

| Component       | Use                                                                                           |
| --------------- | --------------------------------------------------------------------------------------------- |
| `Button`        | Primary action (at most one per area), neutral secondary action, or danger (deletes for good) |
| `IconButton`    | Secondary action, always labeled; toggles on and off; `quiet` for card actions                |
| `TextField`     | Single-line input; Esc leaves the field                                                       |
| `Menu`          | In-panel menu of items with optional check and shortcut                                       |
| `ConfirmDialog` | In-panel question before a costly action, focus on Cancel                                     |
| `Notice`        | Amber-tinted message with an icon and actions; `attention` for Undo                           |
| `Icon`          | Lucide stroke, text color, width 2                                                            |

Menus, dialogs and popups are drawn in the panel's WebView: no native popup takes the panel's focus.
The CSP forbids inline styles, so no component depends on a `style` attribute.

## States

- **Hover:** lighter surface.
- **Focus:** 2 px accent outline, keyboard only.
- **Selected or enabled:** `--color-selected` background with accent text.
- **Empty:** one sentence explaining what to do and the action immediately below.
- **Error:** in red, near the cause, saying what happened.

## Motion

- The panel slides in 150 ms and the edge handle in 100 ms, with a cubic ease-in-out (`motion.rs`).
  CSS transitions use `--duration`; reduced motion removes them.
- Nothing animates for decoration or delays access. The one exception that calls for attention is the
  delete notice (see "Deleting").

## Text

- English by default and pt-BR, in `src/lib/i18n.ts`; tray labels in `tray.rs`. Short and direct.
- Buttons with infinitive verbs: "Open", "Choose folder".
- No exclamation marks, technical jargon or blaming the user.
- Tooltips and accessible labels include working shortcuts, such as "Pin (Ctrl+D)".

# Behavior

## Panel and edge handle

- **Opening:** touching the screen edge beside the handle (a centered stretch of 300% of the tab
  height, clipped to the work area) opens the panel after 150 ms. A rush, reaching the edge at 500
  logical pixels per second or more, opens it at once: in real use the pause only slowed an
  intentional move, so the threshold is low on purpose. The shortcut (Ctrl+Alt+Space by default)
  opens it and goes to the search bar with its text selected.
- **Closing:** without focus (hover only), it closes when the cursor leaves; passing over the taskbar
  or an app bar in the panel's columns does not count as leaving, but clicking there does. Clicking
  the panel or using the shortcut gives it focus; it then stays until focus moves to another app, or
  Esc or the shortcut from the search bar. A pinned panel closes only by unpinning or the tab: Esc or
  the shortcut from its search bar returns focus to the previous app and it stays shown.
- **Focus band:** while the panel or its page has focus, a 2 px `--color-accent` band runs the full
  height of the inner edge, over the border, without moving anything.
- **Side and width:** right by default, or left (Preferences or tray), mirroring everything. 460
  logical pixels wide by default, at least 320 and at most half the work area; a 6-pixel grip on the
  inner edge resizes it (arrows when focused, double-click restores the default). The native page is
  clipped two logical pixels inside the panel's sides and bottom so the band stays visible.
- **Edge handle:** a tab that bulges out of the panel's inner edge, 20 logical pixels deep and 152
  tall, joined by S-curves 28 pixels tall, filled with `--color-bg` and without a line between tab and
  panel. The inner-edge line (1 px `--color-line`, or the focus band) runs around its outline. The
  user's wordmark (`src-tauri/icons/source/wordmark.svg`), 52 pixels long, reads from bottom to top,
  in `--color-fg` when pinned or focused, otherwise `--color-muted`. Closed, it slides in, vertically
  centered on the work area, while the cursor is within 400% of the tab height of it (or beside the
  activation stretch, if longer), and slides out when the cursor leaves; a rush skips it. Open, it
  slides with the panel; a click released on its shape closes the panel, even pinned. No hover state,
  tooltip or focus; dragging does nothing.

## Top bars

- **Top bar:** subtle Notes and Browser tabs, then, in Browser, back/forward and the Reader view
  toggle (an open book, pressed while reading), then the Preferences gear (Ctrl+,) and the panel pin
  (Ctrl+Shift+D), filled when active and 12% smaller than other icons. No app name or page title.
  Ctrl+Tab switches Notes/Browser anywhere in the panel, including the editor and the page.
- **Second bar:** the same search field and "+" action in both modes. In Browser a quiet shield sits
  left of the address field, its icon at the bar's content edge.
- **Shield:** shield means content blocking on, shield-off allowed. One click toggles the whole
  registrable site and reloads the page, without a popover. The tooltip names the site and state.
  Hidden without an HTTP(S) page, disabled while saving.

## Unified address bar

- **Notes mode:** the search persists until the user replaces or deletes it; Esc, creating a note,
  opening a page, hiding the panel and focus loss never clear it. Returning to the bar with Esc or the
  shortcut selects its text. Enter always activates the selected entry. URL-like text offers an
  explicit Open entry after notes, selected by default only with an http(s) prefix; otherwise
  matches, then Create, keep priority, including names such as `todo.md` and `1.5`. Other text offers
  "Search the web", which, like typing `?`, enters `?` mode with remote suggestions. Ordinary note
  searches stay local unless "Web suggestions while searching notes" is on: then engine phrases appear
  after 300 ms and at least three characters of free text, never for filters or while editing. Web
  actions follow the notes as a last "Search the web" section, with note row geometry.
- **Browser mode:** the bar shows the page URL; typing offers Open/Search, matching local addresses,
  then engine suggestions. With suggestions closed, ↓ opens the five most recent local addresses
  without asking the engine. Suggestions float below the bar on one solid surface, with icons for
  search, history and Open; the page is cut out behind them without moving. Esc restores the URL and
  dismisses suggestions before the next Esc closes the panel. Untrusted suggestions appear as text.

## Note list

One list fills the panel under the search bar. Every note is the same card component; pinned and other
notes differ only in section and starting state.

- **Sections:** titles in small semibold primary text, uppercase, slight letter spacing. "Pinned"
  cards in `_pinned.md` order, then "Notes" as one-line rows. Typing filters both in place; pinned
  matches stay cards. The root `_pinned.md` is hidden from list and search. A "Create “…”" item, first
  under the bar, appears when no visible title matches exactly; the first match is selected, so Enter
  expands it and Ctrl+Enter or ↑ reaches Create.
- **States:** line (title, modified date and first tag, +N for more); preview (about eight lines,
  fading at the bottom only when clipped; the default for pinned cards); expanded (full content, no
  height limit or inner scroll, sticky header); editing. Only line versus open is remembered across
  restarts, with the notes folder. One card is expanded at a time; expanding another returns the
  previous one to preview. A Notes row expands in place and returns to a row when closed; there is no
  separate note view. Opening grows the card from its top while the entries below slide (translation
  only); a card whose end falls below scrolls into view in twice `--duration`, never pushing its top
  out. Hiding the panel or losing focus keeps the expanded card, the search and the scroll; returning
  to the search bar does not scroll the list.
- **Mouse:** the header and the chevron toggle line and open; the preview body and "Show all" expand
  the card; an expanded pinned card ends with "Show less". Links work in reading.
- **Names:** a card shows the frontmatter title, else the file name. When the file name differs from
  the title beyond characters a file name cannot hold, the file name follows the title in muted text.
  A note in a subfolder shows its folder path before the title, muted and regular, except inside a
  folder group that names it.
- **Content:** rows and lines show a snippet with matches highlighted; previews and expanded cards show
  the content, hiding the frontmatter title (the header has it). The expanded card lists every tag.
- **Selection:** a line keeps the selected background, accent title and accent border; an open card
  only the accent border; the editing card none, since its border marks it. The chevron, in the
  gutter, points right for a line and down for an open card, and shows on hover, selection and
  keyboard focus; its space is always reserved.
- **Header actions:** no expand or collapse button. From right to left: pin (a bookmark, filled when
  pinned, distinct from the panel's thumbtack), then the pencil when expanded, and the globe "Open
  page" for a note with an http(s) `url:`. `--space-1` gaps, `--size-control` targets, muted icons
  without border or background; hover changes only foreground contrast. They show on hover, selection
  and keyboard focus, and always while editing, with the pencil in accent. On hover, focus or
  selection the inset row box appears, the title slides right and metadata slides left, while actions
  fade in, using only transform and opacity over `--duration`; ellipsis survives in both states.
  Hovering a date shows the full local date and time.
- **Order:** a muted, borderless text button at the right of the Notes header shows Name, Date
  modified or Date created with ↓ (newest first, Z–A) or ↑. Its menu has the six orders with a check,
  a separator and the checkable "Group by date" and "Group by folder" (checking one clears the other);
  it flips above its button near the bottom and returns focus on keyboard dismissal. Preferences >
  General has the same control. Default: modified, newest first. Only unfiltered Notes are sorted;
  searches keep relevance and Pinned keeps file order. Creation time comes from the filesystem.
- **Dates:** absolute by default; "Use relative dates" in Preferences shows now, minutes and hours
  within today, then calendar days, weeks from 7 days, months from 30 and years from 365. One
  30-second tick refreshes dates and groups.
- **Date groups:** off by default; only unfiltered Notes ordered by date. Name order keeps the choice
  but disables the item with a hint. Quiet, regular-weight sentence-case labels: Today, Yesterday,
  Previous 7 days, Previous 30 days, earlier months of this year, then earlier years, by local
  calendar date; ascending order puts older groups first. Not selectable.
- **Folder groups:** off by default; unfiltered Notes in any order. Root notes first without a label,
  then each folder labeled with its path, a subfolder after its parent. Labels appear for folders with
  notes at any depth (pinned included) or completely empty ones, not for folders holding only other
  files. Arrows select labels too, with a spoken path; selectable labels gain a triangle and a
  `--space-3` inset on hover, focus or selection, accent when selected. A label's menu (right click,
  Shift+F10, Menu key) has "Delete folder" (see "Deleting").
- **Unreadable folders:** an amber `Notice` under the search bar says "Some folders could not be
  opened, so their notes may be missing or out of date.", with the paths muted below; notes known from
  them stay. Its close button folds it into a quiet alert icon at the end of the bar, after "+", which
  reopens it with Space; the bar keeps its height. The choice lasts for the session; the notice leaves
  once every folder opens.
- **Folder away:** an unreachable notes folder replaces the list with the empty state: one sentence,
  the path muted and Choose folder. "Unavailable" names the usual causes (a locked vault, an unplugged
  drive, an offline share) and says it opens again when back; "not found" adds that a moved folder is
  chosen again; "not the folder you chose" says nothing is written to it, a locked vault's folder
  opens again once unlocked, and notes moved there are accepted with "Use this folder" or by choosing
  it again. List, previews, pinned items and tag suggestions disappear. An editor with unsaved text
  stays as the only card, with a notice that the text is saved when the folder is back, and Discard my
  changes; Preferences shows the same sentence under the path. With a vault file configured, these
  places also offer Unlock.
- **Web pins:** one-line cards in Pinned: link label as title, host as secondary text, no chevron or
  preview. Click or Enter opens the page in the panel; the bookmark removes it. New pages are saved as
  notes instead.
- **Missing pinned notes:** a "note not found" row with a quiet remove icon; Enter or Delete removes it.
- **Reorder pins:** drag a pinned header: a native image of the card follows the pointer, a dashed gap
  marks the drop position, and the other cards move once the pointer crosses a slot midpoint, with an
  eased transform over `--duration`. Only dropping saves; Esc or dropping outside restores. Or
  Alt+Shift+↑/↓ on the selected pinned item, which stays selected. Unavailable while editing; a stale
  file gives a nearby error.

### Creating and editing

- **Creating:** "+" creates a note in Notes (typed text becomes its title, otherwise Untitled) or saves
  the current page in Browser: title, URL and quoted selection, or the whole article in the reader
  view. Every way of creating opens the new note expanded and editing.
- **Editing:** the pencil, Ctrl+E on the selected card or a double click on its text (which selects
  the clicked word, kept where it was on screen). One card edits at a time and shows its own save
  status, conflict and errors. If the search would hide it, it stays at the top until editing ends.
  Closing the card, expanding another, typing in the search or hiding the panel first leave editing,
  which saves; on a conflict or error nothing else happens until it is resolved.
- **Renaming:** while editing, the title is a borderless field; focused, the line under the header
  turns accent. F2 on the selected card or in its editor enters it with the title selected; Enter, ↓
  or Esc (which restores the title) go on to the text, and leaving the field renames. The file name
  follows the title in the same folder, with " 2" and so on when taken; when it cannot hold the title,
  the title is also written to the frontmatter. A note still named `Untitled` or `Untitled N` is named
  on leaving the editor from the frontmatter title, else a first heading, else its first words (cut
  near 60 characters).
- **Card menu:** right click on a note card, Shift+F10 or the Menu key opens Rename (F2), Pin or Unpin
  (Ctrl+D), Move (Ctrl+M), a separator and Delete (Ctrl+Delete), shortcuts muted at the right. At the
  pointer, or under the header from the keyboard. Text fields, the editor and selected reading text
  keep the native menu; web pins and missing items have none.
- **Moving:** Move, or Ctrl+M from the search bar or list, opens a picker placed and dimmed as a
  dialog: "Move “title”", a focused field and a fixed-height list of "Root folder" and every folder,
  filtered as typed, with a check on the note's own folder; "Create folder “name”" with a plus icon
  ends the list when none matches, and `/` nests. Arrows and the pointer move the highlight, Enter or
  a click takes it, Esc or the scrim cancels. The note keeps its file name (or " 2"), stays selected,
  and an edited note stays in its editor. Folders a move leaves empty stay.

### Deleting

- **Notes:** Delete in the menu, or Ctrl+Delete on the selected card from the list (not the search bar
  or editor, where it deletes a word), deletes at once, without asking; an edited note leaves editing
  first. A `Notice` under the search bar, amber with a trash icon, a neutral Undo and a quiet close,
  says "“title” moved to the Recycle Bin." (Undo until the panel closes) or, where the bin refuses it
  (a vault, a network folder), "Deleted “title”. It stays in the trash for 30 days (Preferences)."
  (Undo while it is in the trash). Images in `attachments/` folders that only this note uses follow
  it, and the notice counts them: "“title” and its 2 images moved to the Recycle Bin." or "Deleted
  “title” and its 2 images. They stay in the trash for 30 days (Preferences)." They actually move
  after the panel hides and the note left open is saved, if no note uses them by then: until then
  Undo needs nothing for them, and one that cannot go stays in place, under "Unused images". It leaves after 10 seconds in sight, or when
  closed, undone or replaced; time stops while hovered, focused or hidden. To be noticed, it slides
  down and fades in over two `--duration`, then Undo pulses twice (accent outline, selected
  background, five `--duration` each); with reduced motion it appears at once with that look. Undo
  brings back the note and its pinned items and selects it; otherwise selection moves to the next
  item. A note over 2 MB is not deleted: the notice says it is too large to keep a copy for Undo and
  to delete it in File Explorer.
- **Folders:** "Delete folder" deletes at once: an empty folder is removed, any other goes whole to the
  Recycle Bin or the trash. A folder with notes shows "Folder “name” moved to the Recycle Bin." with
  only a close button, or the trash text with Undo. Root has no label to delete.

### Keyboard

- **Tab stops:** search bar → "+" → folded unreadable-folders icon → recovery actions when shown (Undo,
  Restore leftovers, Restore pinned items; in Browser, the permission bar's Block and Allow) → list →
  order button → top bar buttons → resize grip, reversed with Shift+Tab. The list is one stop;
  entering without a selection selects the first item, shown only by the card's selection style. Card
  controls and label buttons stay clickable and labeled outside the Tab sequence. The list is a
  labeled region of plain groups, not a listbox; the selected item has `aria-current` and a polite
  live region announces it.
- **List keys** work from the search bar and from anywhere in the panel except a text field or the
  editor, where typing a character goes to the search bar. Enter acts on the selection even on a
  focused button; Space presses the button. → and ← in the bar act on the card only with the caret
  at the end of the text.
- **Esc** leaves one layer at a time: the image preview, editing, then back to the bar with its text
  selected, then from the bar it closes the panel.

| Shortcut                  | Action                                                                                                 |
| ------------------------- | ------------------------------------------------------------------------------------------------------ |
| ↑ / ↓                     | Select a card, folder label or row; on a long expanded card, first scroll about three lines to its end |
| Alt+↑ / Alt+↓             | Select the previous / next item directly                                                               |
| Alt+Shift+↑ / Alt+Shift+↓ | Move the selected pinned item up / down                                                                |
| Ctrl+↑ / Ctrl+↓           | Jump to the first note of the previous / next section (Pinned, Notes, each group); list focus only     |
| PgUp / PgDn               | Scroll the list by about one screen                                                                    |
| → / ←                     | Open / close one level: line, preview, expanded                                                        |
| Enter                     | Expand the selected item, run "Create", or remove a selected missing pinned item                       |
| Delete                    | Remove the selected missing pinned item                                                                |
| Ctrl+Delete               | Delete the selected note; outside text fields                                                          |
| Ctrl+M                    | Move the selected note to a folder; from the search bar or the list                                    |
| Shift+F10 / Menu          | Open the selected note or folder menu                                                                  |
| Space                     | On a focused grid thumbnail, show or hide it beside the panel                                          |
| Ctrl+Enter                | Create a note with the exact typed text                                                                |
| Ctrl+D                    | Toggle pin on the selected card from search or the list; in the editor, see below                      |
| Ctrl+Shift+D              | Toggle the panel pin                                                                                   |
| Ctrl+,                    | Open / close Preferences                                                                               |
| Ctrl+/                    | Open / close the shortcut sheet                                                                        |
| Ctrl+E                    | Toggle editing on the selected card                                                                    |
| F2                        | Rename the selected card                                                                               |
| Ctrl+F                    | Focus the search bar and select its text; the editor finds in the note first                           |
| Esc                       | Leave editing; next, return to the bar and select its text; from the bar, close                        |
| Ctrl+Alt+Space (default)  | Open and go to the search bar with its text selected; from the search bar, close                       |

### Search filters

- **Tags:** whitespace-delimited `#tag` tokens must each match a complete frontmatter tag, ignoring case
  and accents, alongside the free text. Body hashtags are ordinary text; a lone `#`, `# Title` and
  hashes inside words or URLs stay literal. Tags with spaces or quotes use a JSON-quoted token, such as
  `#"two words"`.
- **Dates:** `date:` offers Today (`date:today`), Last 7 days (`date:7d`), This month (`date:month`)
  and Last month (`date:last-month`), with localized labels. Years, months, days and inclusive ranges
  also work: `date:2026`, `date:2026-10-03`, `date:2026-10..2026-11`, `date:2026-10..`, `date:..2026`.
  The date is the frontmatter `created`, else `date`, else the modified time. Invalid or reversed
  tokens are ignored. All filters and free-text words apply together.
- **Completion:** while the caret is in a token, matching tags or dates appear below the bar in the
  suggestion style: arrows select, Enter or Tab inserts the token and a space, a click accepts, Esc
  dismisses first. The partial token is ignored for filtering until accepted.
- **Creating:** queries with filters never offer automatic Create. Ctrl+Enter and "+" create from the
  free text only (untitled for tags only), seeding the tags in the frontmatter so the note stays in
  the results; dates are ignored. The web search row offers only the free text.

## Note editor

- **Place:** the editor lives in the card, in place of its content; no separate view or toolbar. The
  card keeps its header; the small Saving… / Saved status sits left of the actions, absent on conflict
  or error, whose notices sit at the top of the body. The editing card has an accent border, and its
  text keeps the reading padding, so toggling does not move it. Reading is the default.
- **Live formatting:** syntax is visible on selection lines and hidden elsewhere. A cursor touching an
  inline code span or fenced block shows both delimiters, including the info string. During a pointer
  gesture the reveal waits, so text does not move under a click.
- **Images:** paste an image or drop image files from Explorer into the editor to insert them at the
  current selection. A drop inserts images in order. Single line breaks join consecutive pastes and
  images dropped together into one paragraph. The editor ignores non-image files.
- **Conflicts:** a primary Keep my version and a secondary Discard my changes above the text; errors in
  red beside it.
- **Footer:** muted word and character counts and reading (300–175 words per minute) and speaking
  (160–130) time ranges, for the selection or the whole note, also for the focused card in reading.
  Frontmatter is excluded.
- **Tags:** light theme uses the selected background and accent text in both views; dark theme keeps
  the surface background and muted text.

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
| Ctrl+/                                        | Open / close the shortcut sheet                                                            |
| Ctrl+;                                        | Add a frontmatter property: a new line in the block, created if needed, with keys offered  |
| /                                             | Open slash commands at the start of a line or after whitespace                             |
| Alt+Shift+A                                   | Cycle lowercase → Sentence case → Capitalize Each Word → UPPERCASE                         |
| Ctrl+E                                        | Toggle reading and editing from anywhere in the panel                                      |
| Ctrl+F / Ctrl+H                               | Find / replace in the note, starting from the selection; Ctrl+F again goes to the bar      |
| Enter / Shift+Enter, F3 / Shift+F3            | Next / previous match                                                                      |
| Ctrl+D                                        | Select the next occurrence of the selection, adding a cursor                               |
| Alt+click                                     | Add a cursor                                                                               |
| Esc                                           | Close the transform menu, completion list or find panel before leaving editing             |

- **Formatting:** inline commands wrap the selection, unwrap matching markers or insert a pair at an
  empty cursor; block commands touch only line prefixes. Every command is one undo step.
- **Pairs:** `(`, `[`, `{` and quotes insert their closer, and typing the closer steps over it. Quotes
  after word characters stay literal (`don't`); backticks stay literal. Backspace removes an empty
  pair first.
- **Formatting bar:** a compact bar floats above a single non-empty selection after mouse release,
  hiding while dragging, on blur or while read-only. Icon buttons: bold, italic, strike, inline code,
  heading 1, bullet list, task list, quote, link and property; the last opens a menu for case changes,
  sorting lines either way and removing duplicate lines. Surface, line and control tokens, no shadow;
  editor focus and selection are kept; buttons stay outside the Tab sequence; the menu takes arrows,
  Home/End, Enter or Space and Esc.
- **Transforms:** case changes use the interface locale on each selection or the word at the cursor,
  keeping Markdown punctuation; the shortcut skips equivalent styles and starts at lowercase for mixed
  case. Line actions expand to whole lines; sorting is numeric and ignores case and accents;
  deduplication keeps the first exact match. Each change is one undo step and selects the result.
- **Slash commands:** `/` at a line start or after whitespace, outside code, URLs and frontmatter,
  opens the quiet completion list, filtered by localized label or English id. Esc, moving away or no
  match closes it and keeps the text. Commands format the line, insert a code block, divider, link or
  local date, or add a property, in one undo step.
- **Find and replace:** one compact row stuck to the top of the editing card: replace toggle, field, a
  muted "3 of 12" count, match case (Aa) and regular expression (.*) toggles, previous, next and close.
  The replace row shows after Ctrl+H or the toggle. Matches look like list search highlights; the
  current one has an accent outline.
- **Frontmatter:** stays YAML text, revealed as a framed raw block on click or keyboard activation.
  Typing at a line start offers missing common keys (title, tags, aliases, date, author, description,
  status, url), and typing a tag offers the folder's tags, in a quiet monospace list. Unreadable tags
  have a muted wavy underline and a tooltip. Leaving editing repairs comma-separated plain words into a
  list and quotes plain `word: word` items in a flow list, as one undoable edit; ambiguous YAML stays
  untouched, and typing never repairs.
- **Spelling:** misspelled words have a wavy danger underline, waiting until the cursor leaves the
  word. A right click on one, outside links, offers up to six suggestions (or "No suggestions") and Add
  to dictionary; other text keeps the native menu.

## Rendered content

- Reading and editing share `features/notes/content.css`, scoped by `.note-content`. List paragraphs
  stay tight, like editor lines.
- Images load lazily. A paragraph containing only two or more local images, separated by whitespace
  or line breaks, shows a grid of square thumbnails. Tab reaches them in the expanded note. A plain
  `<br>` counts as a line break in a grid. Raw HTML elsewhere shows as plain text.
- **Image preview:** hovering a grid thumbnail for 300 ms, or focusing it and pressing Space, shows
  the image larger in a window beside the panel's inner edge, past the edge tab, vertically centered
  on the thumbnail and 24 logical px from the work area's other edges. It takes at most 72% of the
  room's width and 76% of its height (never under 120 px), at most the image's natural size and
  never upscaled. The frame is styled like the panel: `--color-bg` with a 1 px `--color-line`
  outline, 8 px corners, `--space-2` around the image (whose corners are `--radius-sm`), no shadow,
  and a callout arrow 8 px deep and 16 px tall on the panel's side, at the thumbnail's height.
  It never takes focus or clicks, so the panel stays open and hovered as before, and it is excluded
  from capture. Once shown, hovering or focusing another thumbnail switches at once; moving between
  thumbnails keeps it, leaving the grid hides it. Esc (its own layer), Space again, a click, scrolling,
  focus leaving the grid, editing, Browser or Preferences, any menu or dialog, the shortcut and hiding
  the panel also hide it. Not shown with too little room beside the panel (120 logical px).
- **Floating images:** a local image, single or a grid thumbnail (not one inside a link), has a
  `pictureInPicture` badge in its top right corner, `--space-1` from the edges, with
  `--color-surface`, a 1 px `--color-line` border and `--radius-sm`, scrolling with the image and
  never moving the note. It shows, outlined, on hover or when the thumbnail has focus, and opens
  the image as a floating window ("Open as floating window"). While a floating window shows the
  image, the badge stays, its small window filled, and closes that window ("Close floating
  window"); the image's menu (right click, Shift+F10 or Menu, named "Image actions") has the same
  toggle, and a middle click on the image opens it. Opening detaches the image into its own window
  where the preview would show it, each later window 24
  logical px lower and away from the panel (left of a panel on the right, right of one on the
  left), staying in the room beside the panel; at most 8 at once ("Up to 8 floating windows can be
  open; close one first." in the danger color otherwise). A floating window is borderless, topmost
  and excluded from capture; clicking or dragging it never takes focus from the app in use, and the
  panel stays as it was. The window keeps the image's proportions, so the image fills it, with a
  transparent `--space-2` gutter all around. While hovered, a hairline (`--color-float-ring`) and a
  soft shadow (`--color-float-shadow`) trace the image's own edges, or a transparent image's
  outline: the identity's one shadow, since the window has no frame to show where it ends. A pill
  of controls appears `--space-2` inside the image's top right corner (`--color-surface`, 1 px
  `--color-line`, `--radius-full`): how large the image shows against its natural size, inner zoom
  included (`--font-size-sm`, `--color-muted`, tabular figures, hidden until the image loads);
  then, past a 1 px `--color-line` divider, a small muted `blend` icon and a short opacity slider
  from 15% to 100% in 5% steps, its thin track `--color-fg` up to the small `--color-fg` thumb and
  `--color-line` after it, no accent; "Let clicks through" as a quiet
  `IconButton` whose pointer icon is outlined when off and filled when on; and Close. Only the
  image fades: the pill stays opaque. Alt+wheel anywhere, or the wheel over the pill, changes the
  opacity. Ctrl+wheel zooms: the window grows or shrinks around its center, stopping at the image's
  natural size (1:1) on the way, up to the side preview's share of the monitor (72% of its work
  area's width, 76% of its height); past that the image zooms inside the window, toward the
  pointer, up to 8×, and zooming out returns to no inner zoom before the window shrinks. While
  zoomed inside, Ctrl+drag pans the image, never past its edges and settling on the center within
  6 px; Ctrl+double-click returns to the whole image at its natural size where it fits, and a
  Ctrl+drag never moves the window. Ctrl and
  Alt together (AltGr) do none of these. Dragging
  the image or the pill's background moves the window; invisible strips 6 px either side of the
  image's edges and corners resize it, keeping its proportions, the opposite edge or corner in
  place, the image's shorter side at least 96 logical px and the window within the monitor's work
  area. Letting clicks through hides the controls and passes every click to the windows below;
  holding Alt (not AltGr) with the pointer over the window reveals the controls and takes clicks
  again, also for a drag started meanwhile, until released. "Unlock floating windows" in the tray,
  available while any window lets clicks through, makes all of them take clicks again, and "Close
  all floating windows", available while any is open, closes them all as their own Close would. Windows
  reopen where they were, at their size and opacity, always taking clicks, at the next start or
  when the notes folder returns; one whose spot is now off every monitor moves onto the nearest. A
  locked vault, or another folder chosen, closes them at once, without forgetting them; a window
  whose image was deleted is forgotten.
- **Floating notes (post-its):** a note card's menu has "Open as floating window", which opens the
  note read-only in a floating window beside its card, 300 by 320 logical px, cascading and limited
  to 8 together with floating images; while one shows the note, the item reads "Close floating
  window" and the card's header actions add a filled `pictureInPicture` that closes it. The window
  behaves as a floating image's (topmost, never taking focus, excluded from capture, transparent
  `--space-2` gutter, hover hairline and shadow, opacity, click-through with Alt, tray items,
  reopening and the vault), on a sheet of `--color-surface` with a 1 px `--color-line` border and
  `--radius-sm`, the note rendered as in the panel, title included, inside `--space-2` by
  `--space-3` padding, scrolling with the wheel. Its pill has the opacity slider, a pencil ("Edit
  in the panel (double-click)"), "Let clicks through" and Close; the whole sheet fades, never the
  pill. Dragging the sheet more than 4 px moves the window; its edges and corners resize it to any
  proportions, each side of the sheet at least 96 logical px. A double click, or the pencil, brings
  the panel with focus to Notes and opens the note expanded in the editor. Ctrl+wheel sizes the
  text in 10% steps from 50% to 300%, kept with the window, and Ctrl+double-click returns it to
  100%. Links read as links but open nothing, and nothing in it takes focus. It follows edits and
  renames as they happen; a note moved or deleted outside its sight leaves only "This note was
  moved or deleted." in the middle of the sheet until it returns, and is forgotten at the next
  start.
- Code: inline code and blocks use the recessed `--color-bg` with `--radius-sm`; inline code has
  `--space-1` at each side; blocks have `--space-2` vertical and `--space-3` horizontal padding and a
  `--color-line` border. The editor frames code lines, fences included while visible, as one block.
- Footnote references and labels are small raised text, references in accent; definitions are small
  muted text where they are written.
- GitHub alerts show the English kind names without icons; Note, Tip and Important use the accent,
  Warning and Caution the danger color.
- Links: a click opens a web link in the panel when reading and places the cursor when editing.
  Shift+click, middle click and, in reading, Ctrl+click open the system browser; email links open the
  mail handler. A right click, or Shift+F10 / Menu on a focused link or at the editor caret, opens a
  menu: Open in browser, Copy link and, for web links, Open here. Tab reaches links in the expanded
  note.
- Copy and cut in the panel put plain text on the clipboard, kept out of clipboard history (Win+V) and
  cloud clipboard, never cleared later. The web page copies as any browser does.

## Browser

- **Start:** Browser starts with a short hint and the field focused; switching tabs keeps the page and
  history.
- **Context menu:** the page keeps only editing items and back/forward/reload, then, after a
  separator, "Capture visible area" and "Capture full page", also on a link, image or selection. They
  are disabled without the notes folder, while the page is unavailable or loading, in the reader view,
  under Preferences, and while a capture, save, note creation or folder choice is pending.
- **Captures:** each goes into a new note titled and sourced from the page. If the panel still shows
  that page in Browser when it is saved, and has not hidden it since, it returns to Notes with the note
  in the editor; otherwise nothing switches and a dismissible notice above the list names the note. A
  page too long for one image is clipped to its start, with a notice. On failure the page stays and
  the error line shows over its top.
- **Reader view:** replaces the page under the same bars with its article in the reading styles: the
  title as header, then text, headings, lists, quotes, code and tables, with its own scroll and no
  images. The toggle is disabled while reading the page; a failure says "No article text was found on
  this page." under the top bar. Links open in the page; "+" saves the whole article. The toggle,
  back/forward, opening a page, switching tabs or Preferences return to the page.
- **Permission requests:** a site asking for the location, camera, microphone or clipboard gets a
  `Notice` over the top of the page, under the address bar, on the panel background with a line below:
  the kind's icon (map pin, camera, microphone, clipboard), "example.com wants to use your location."
  and, when a frame of another origin of the site asked, a muted line naming it; then neutral Block and
  Allow, neither primary. The page is cut out behind it. Focus stays where it was, and the panel stays
  open until an answer; Notes, Preferences, the reader view, closing the panel or leaving the page deny
  it without remembering. Requests show one at a time; each answer is remembered for the site.

## Preferences

The gear opens a third view with quiet General, Appearance, Browser, Shortcuts, Trash and About section
buttons, wrapping when narrow. The page stays loaded but hidden. The gear or Esc returns to the
previous tab; either tab also leaves, and the global shortcut returns to Notes and search. Sections
scroll inside the panel with the same insets, tokens and neutral buttons, and no heading repeating the
tab label.

- **General:** folder path and picker, optional vault file, note order, date grouping, relative dates,
  "Web suggestions while searching notes" (off by default, with a hint that typed searches go to the
  provider), "Check spelling" (on by default, with the languages found and where Windows adds more),
  panel side and "Start with Windows". Side and startup mirror the tray, also while open; startup is
  read from Windows, disabled until read, and keeps showing the actual state after a failure.
- **Vault file:** its path, or a short optional hint, with Choose file (filtered to `.cryptomator`) and
  Clear. Unlock opens it in its app, where Cryptomator handles authentication, including Windows
  Hello. A missing file shows a nearby error asking to choose it again. The folder returns without
  another action once unlocked.
- **Appearance:** System, Light and Dark radio buttons, applied at once; the panel width, how to resize
  it and a reset.
- **Browser:** the search provider (DuckDuckGo by default; Custom reveals a URL field and Save, and an
  invalid URL shows a hint and searches in DuckDuckGo). Then Allowed sites with Remove, a hint on the
  subdomain scope and, when empty, how to add one with the shield. Then site permissions in the same
  rows: the site and, muted below, "Location · Allowed", with Remove; the hint says answers cover
  subdomains and removing asks again, and the empty list says a bar above the page asks.
- **Shortcuts:** the activation shortcut in a field-like box with Change. Recording: the box asks for
  a combination and shows held modifiers, the button becomes Cancel, current shortcuts stand aside, and
  the next non-modifier key ends it, shown as the layout gives it (A for AZERTY's Q key). Esc, Tab,
  leaving or hiding the panel, or 30 seconds cancel. A refused combination (no Ctrl, Alt or Win;
  Ctrl+Alt with a letter or digit, which is AltGr on layouts such as ABNT2; a key other than a letter,
  digit, F1–F24, arrow, Space or navigation key) or one taken by another app keeps the previous one and
  says why in the danger color. If the saved one fails at startup, the default is used and a
  dismissible notice under the top bar names both. Below, the same key list as the Ctrl+/ sheet.
- **Shortcut sheet:** Ctrl+/ anywhere in Notes opens a quiet, scrollable dialog with surface and line
  tokens, no shadow or dimming. Focus stays inside; Esc, Ctrl+/ or close restores the previous focus,
  including the editor cursor.
- **Trash:** a muted paragraph says items the Recycle Bin cannot take stay 30 days inside the notes
  folder (encrypted in a vault) and synced with it. Rows newest first: the name and, muted below, its
  folder ("Notes folder" for the root) and relative deletion date (full date on hover), with Restore,
  which says "Restored as path." since a taken name gets a free one. "Empty trash" (danger) asks with
  the item count, confirmed by "Delete for good", the only permanent action and so the only question.
  Empty: "The trash is empty."; folder away: the trash appears when it is back. An entry that cannot
  be identified shows "Unrecognized item" without Restore. Restoring a note brings back the images
  that followed it; one whose name is now taken, or that cannot move, stays, and an error line counts
  and names them ("This image could not return…" or "These 2 images could not return…"). Such images
  show as one row named after the first of them, dated from that restore, and Restore brings them
  all. The
  trash never shows in the list.
- **Unused images**, under its own heading below the trash: rows show lazy thumbnails, filenames and
  their folders, with checkboxes to select individual images or all of them. Send to bin moves the
  selection through the same bin and fallback as Trash. Empty: "No unused images."; folder away: the
  images appear when it is back. An image a floating window shows, open or to reopen, is in use. For
  the candidate and reference rules, see [Unused images in Architecture](ARCHITECTURE.md#unused-images).
- **About:** the installed version, live update status and last successful check. Check now starts a
  check; development builds say updates are disabled. A verified download shows Restart to update and
  enables the tray action and tooltip. Nothing moves the panel or focus.

## Tweak window

Development builds only: a small Tweak button at the bottom of the panel opens an ordinary resizable
window that can stay on another monitor while the panel behaves as in production. Every change applies
live to the panel, even hidden, in both color schemes. A look becomes the identity only when the user
applies its export, a set of unified diffs for the source files.

- **Layout:** options scroll beside an always-visible WCAG AA/AAA contrast table, stacked in narrow
  windows. Collapsible groups: typography, colors, layout, CSS transitions, slides, edge handle, tab
  shape, presets and export.
- **Numbers:** apply on input, with bounded steppers and Up/Down; Shift multiplies the step by ten.
- **Colors:** HSB by default, a swatch beside the CSS/hex field and alpha; untouched source spellings
  stay unchanged.
- **Fonts:** each font token has a filtering combobox: curated stacks, then installed families, each
  with a sans-serif or monospace fallback.
- **Slides:** duration and `cubic-bezier` curve for the panel and the handle, with presets (overshooting
  `back` curves included), four numbers and a draggable curve editor; a slide reads its motion when it
  starts.
- **Edge handle:** activation height (100–1000% of the tab), reveal radius (10–1000%) and rush speed
  (500–20000 logical pixels per second).
- **Tab shape:** depth (4–64), height (40–400) and curve height (at most half the height), applied at
  the next cursor poll; the wordmark length (10–300) is UI-only.
- **Presets:** named sets for A/B switching; rename, delete or overwrite, the last two with inline
  confirmation. Reset restores the source values. The window remembers its state across restarts.
- **Limit:** the page window uses a fixed bar offset in Rust, so another bar size needs a code change.
