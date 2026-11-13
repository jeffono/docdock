# Design

Identity chosen by the user: a neutral, compact system utility. Values live in
`src/lib/styles/tokens.css`; this file says how to use them. Only the user changes the identity.

## Principles

- **Quick reference:** the panel is narrow and opens briefly. Content first, with minimal controls.
- **Density:** more content per screen, without crowding the text.
- **Subtle:** color is a signal, not decoration.

## Color

- Neutrals for almost everything. Blue accent marks only selection, enabled state, links, focus and the primary action.
- Light and dark themes follow Windows by default, with a System, Light or Dark choice in Preferences.
  Every new color starts in both themes, in `light-dark`.
- Red only for errors. Fixed colors outside `tokens.css` fail `check:fast`.
- Amber (`--color-warning*`, chosen by the user) only tints notices that need attention without being
  errors, such as Undo after a delete. A light scrim (`--color-scrim`) dims the panel only behind a
  confirmation dialog.
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

| Component       | Use                                                                                           |
| --------------- | --------------------------------------------------------------------------------------------- |
| `Button`        | Primary action (at most one per area), neutral secondary action, or danger (deletes for good) |
| `IconButton`    | Secondary action, always labeled; toggles on and off                                          |
| `TextField`     | Single-line input; Esc leaves the field                                                       |
| `Menu`          | In-panel menu of items with optional check and shortcut                                       |
| `ConfirmDialog` | In-panel question before a costly action, focus on Cancel                                     |
| `Notice`        | Amber-tinted message with an icon and actions; `attention` for Undo                           |
| `Icon`          | Lucide stroke, text color, width 2                                                            |

## States

- **Hover:** lighter surface. Note metadata slides aside to reveal header actions.
- **Focus:** 2 px accent outline, keyboard only.
- **Selected or enabled:** `--color-selected` background with accent text.
- **Empty:** one sentence explaining what to do and the action immediately below.
- **Error:** in red, near the cause, saying what happened.

## Motion

- Panel slides in 150 ms, defined in `dock.rs`. Hover uses `--duration`.
- Nothing animates for decoration or delays access. The one exception that calls for attention is the
  delete notice's entrance and Undo pulse, described under Deleting.

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

- **Sections:** section titles use small semibold primary text, CSS uppercase and slight letter spacing.
  "Pinned" cards in `_pinned.md` order, then "Notes" as one-line rows. Typing filters
  both in place and pinned matches stay cards. The root `_pinned.md` file is hidden from the list and
  search. A "Create “…”" item appears when no visible note title matches exactly.
- **Order:** a muted, borderless text button at the right end of the section header shares the
  title baseline. It shows Name, Date modified or Date created with ↓ for descending (newest first,
  Z–A) and ↑ for ascending (oldest first, A–Z), matching the menu. Its in-webview menu and the same control in Preferences > General share
  one saved choice: name (display title), modified or created, ascending or descending. The default is
  modified, newest first. Only the unfiltered Notes section is sorted; searches keep relevance and
  Pinned keeps file order. Creation time comes from the filesystem, falling back to modified time.
- **Folder away:** a notes folder the app cannot reach replaces the list with the empty state: one
  sentence, the folder path in muted text and Choose folder. "Unavailable" names the usual causes (a
  locked vault, an unplugged drive, an offline share) and says the folder opens again when it is back;
  "not found" (the folder above it is there) adds that a moved or renamed folder is chosen again;
  "not the folder you chose" (a folder is at the path but its identity marker is not the chosen one's)
  says nothing is written to it, a locked vault's folder opens again once unlocked, and notes moved
  there are accepted with "Use this folder" or by choosing the folder again. "Use this folder" appears
  in both the list empty state and the editor notice for an unrecognized folder. The
  list, previews, pinned items and tag suggestions disappear with it. An editor holding unsaved text
  stays as the only card, with a notice at the top of its body that the text stays there and is saved
  when the folder is back, and Discard my changes; Preferences shows the same sentence under the path.
- **Order menu:** six single-choice items show a check on the current order; a separator precedes
  the checkable "Group by date" and "Group by folder" items; checking one clears the other. Enter/Space and arrows open and navigate it; Esc, selection and
  outside click dismiss it. Keyboard dismissal and selection return focus to its button. The menu
  stays within the panel, flipping above its button near the bottom. No native popup takes focus.
- **Note dates:** modified dates on cards and rows are absolute by default. General Preferences
  offers "Use relative dates", remembered on this computer. Relative text uses the app locale: now,
  minutes and hours within today; calendar days across midnight, then weeks from 7 days, months
  from 30 days and years from 365 days. One shared 30-second tick refreshes dates and date groups.
  Hovering a note date shows the full local date and time with either display option.
- **Date groups:** the optional menu item defaults off. It groups only unfiltered Notes when ordered
  by modified or created date. Name order keeps the choice but disables the item with a visible hint.
  Quiet, regular-weight labels keep sentence case below the stronger section titles:
  Today, Yesterday, Previous 7 days (2–7 days ago), Previous 30 days (8–30 days ago), then earlier
  months of this year (month names in the user's locale), then earlier years. Boundaries use local
  calendar dates; ascending order puts older groups first. Labels are not interactive or selectable.
- **Folder groups:** the other optional item, also off by default, groups unfiltered Notes in any
  order. Root notes come first, right under the section title and without a label; then each folder,
  labelled with its path, by path segment, so a subfolder follows its parent. Each group keeps the
  chosen order. Labels appear only for folders containing notes directly or below (including pinned
  notes), or completely empty folders. Folders containing only images or other files have no label
  and cannot be deleted from the app. Tab enters the list once; arrows select folder labels as well
  as cards, with a spoken path. Selectable labels gain a discreet triangle
  and an inset of `--space-3` on hover, focus or selection, with no background, higher text contrast
  (accent when selected) and transitions
  over `--duration` (ease-out for the inset and triangle). Reduced motion removes transitions.
  Date labels and the unlabelled root keep their existing appearance. Label buttons stay outside the Tab sequence; right click, Shift+F10 or the Menu key opens one "Delete folder"
  item without a shortcut. It deletes at once, without asking: an empty folder is removed, any
  other goes whole (hidden files and attachments included) to the Recycle Bin, or to the trash inside
  the notes folder where the Recycle Bin refuses it. A folder with notes then shows the deletion notice:
  "Folder “name” moved to the Recycle Bin." with only a close button, since it is restored there, or
  the trash text with Undo, as for a note. Root has no deletion label. Folders left empty by moves or note deletion stay.
- **Reorder pins:** drag a pinned header to move it to another pinned position. A native image of
  the whole card follows the pointer; a dashed gap marks its pending position while the other cards
  stay still until the pointer crosses a neighbouring slot midpoint, then move once with an eased
  transform animation over `--duration`. Slots use captured card heights and the current preview
  order, independent of animated hit targets; gaps accept the shown drop position.
  Only dropping saves the order; Esc or dropping outside
  restores it. Reduced motion disables the animation. Note bodies keep text selection. Or use
  Alt+Shift+↑ / Alt+Shift+↓ on the selected pinned item (including web and missing items). Selection
  follows the item. Reordering is unavailable while editing. A stale file produces a nearby error.
- **Web pins:** one-line cards in the Pinned section, in file order alongside notes: link label as
  title, host as secondary text, no chevron or preview. Click or Enter opens the page in the panel;
  the bookmark removes it. Existing web pins remain usable; new pages are saved as notes and pinned from their cards.
- **Page notes:** the shared bar offers a "+" action: create a note in Notes (typed text becomes its title, otherwise Untitled), or save the current page in Browser. It saves the page title,
  URL and quoted selection (in the reader view, the whole article instead), then returns to notes with the new card expanded and editing, ready for
  annotations. A note with an http(s) `url:` has an "Open page" globe action left of the pencil; it
  keeps the usual expand/edit behavior and header action visibility. Other URL schemes have no action.
- **States:** line (title, modified date and first tag, with +N for more tags); preview (up to about
  eight lines, fading at the bottom; the default for pinned cards); expanded (full content, reading);
  editing (expanded, with the editor). Only line versus open is remembered across app restarts, per
  computer. Hiding the panel leaves editing through its save guard, keeping the card expanded.
  The expanded card and list scroll survive hiding and focus loss; only collapsing the card or
  expanding another changes the expanded note. Selection navigation and explicit opening may reveal
  a card, but returning to the search bar does not scroll the list.
- **Expanded:** no height limit and no inner scroll. While scrolling inside the card, its header
  (title and actions) sticks to the top. Only one card is expanded: expanding another returns
  the previous one to preview. Clicking a Notes row turns it into an expanded card in place, and it
  returns to a row when closed; there is no separate note view. Opening grows the card from its top
  edge while the entries below slide (translation only, never scaling). An opened card whose end falls
  below the list scrolls into view in twice `--duration`, never pushing its top out.
- **Renaming:** while editing, the card title is a borderless field in place of the title and
  metadata; when it has focus, the line under the header turns accent instead of a nested focus ring.
  F2 on the selected card, or in its editor, enters editing with the title selected. Enter, ↓ or Esc
  (which first restores the title) go on to the text; leaving the field renames, and an empty or
  unchanged title does nothing. The file name follows the title in the same folder, with " 2" and so
  on when taken; a frontmatter title changes with it. When the file name cannot hold the typed title
  (characters such as ":", or a taken name), the field also writes it as the frontmatter title,
  creating the block if needed, so the card keeps showing what was typed. A double click on the title only opens and
  closes, as a click does; a double click on the text of a card enters editing. A note still named
  `Untitled` or `Untitled N` is named when its editor is left, from what the card shows: the
  frontmatter title, else a first heading, else its first words (cut at a word near 60 characters).
- **Names:** a card shows the frontmatter title, else the file name. When the file name differs from
  the title beyond characters a file name cannot hold (a taken name got " 2", or the file was renamed
  elsewhere), the file name follows the title in muted text, so the two notes stay apart and the rule
  shows itself without any explanation. A note in a subfolder shows its folder path before the title,
  muted and in regular weight, except inside a folder group that already names it.
- **Editing:** The browser "+" opens a newly captured page in edit mode. Otherwise use the pencil in
  the card header, Ctrl+E on the selected card or a double click on its text; entering from preview
  expands the card. One card edits at a time and shows its own save status, conflict and
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
  expanded, with `--space-1` gaps and `--size-control` click targets. Actions show only on hover, on the
  selected item and on keyboard focus. At rest, pinned metadata has the same border inset as the title. Row titles align with section
  titles and date-group labels; their metadata ends at the order button text edge. On hover, focus
  or selection the existing inset row box appears, the title slides right and metadata slides left
  into its inner padding. These transitions share the actions' duration and easing.
  Metadata slides left and actions fade in over `--duration` using only
  transform and opacity; title clearance prevents overlap and preserves ellipsis in both states.
  The slide distance includes every visible action, including the globe and expanded card's pencil.
  Reduced motion disables these transitions. Icons use `--size-icon`, muted color,
  no border and no background; the bookmark is filled when pinned and outline otherwise. While
  editing, all header actions stay visible and the pencil uses the accent color, still without border or background. Icon hover changes only foreground contrast; the row hover surface covers exactly the same inset
  and radius as its selected surface. Tooltips and accessible labels include shortcuts:
  "Pin (Ctrl+D)" / "Unpin (Ctrl+D)", "Edit (Ctrl+E)", "Open (→)" / "Close (←)" and "Remove from
  pinned (Delete)".
  Headers have `--space-1` inner padding on every side in every state; missing-note rows use the same
  vertical and right inset and a quiet remove icon.
- **Card menu:** a right click on a note card, Shift+F10 or the Menu key on the selected card opens
  the same in-panel menu as the order menu: Rename (F2), Pin or Unpin (Ctrl+D), Move (Ctrl+M), then
  after a separator Delete (Ctrl+Delete), each with its shortcut in muted text at the right. A right click opens it at the pointer, kept inside the panel; the keyboard opens it under
  the card header. Text fields, the editor and selected reading text keep the native menu. Web pins
  and missing items have none.
- **Moving:** Move in the card menu, or Ctrl+M on the selected card from the search bar or the list
  (not the editor), opens a picker placed and dimmed as the delete dialog: the question "Move “title”",
  a field with the focus, and a list of fixed height below it. The list holds "Root folder" and every
  folder, filtered as the field is typed (ignoring case), with a check on the note's own folder; when
  no folder has the typed name, "Create folder “name”" with a plus icon ends the list. `/` nests a new
  folder. ↑ / ↓ move the selected-surface highlight, the pointer follows it, and Enter or a click
  takes it; Esc or a click on the scrim cancels, and focus returns to the list. Taking the note's own
  folder does nothing. The note keeps its file name, or " 2" and so on where it is taken, and stays
  selected; an edited note stays in its editor. Folders a move leaves empty stay.
- **Deleting:** Delete in the card menu, or Ctrl+Delete on the selected card from the list (not in
  the search bar or the editor, where it deletes the next word), deletes at once, without asking. An
  edited note leaves editing first, which saves. Then a `Notice` under the search bar, on the amber
  tint with a trash icon, says where the note went, with a neutral Undo button and a quiet close
  button: "“title” moved to the Recycle Bin." (Undo until the panel closes), or, where the Recycle Bin
  refuses it (an encrypted vault, a network folder), "Deleted “title”. It stays in the trash for 30
  days (Preferences)." (Undo while the item is in the trash, also after the panel closes). It leaves
  after 10 seconds in sight, or when closed, undone or replaced by another delete; time stops while the
  pointer or focus is on it and while the panel is hidden or Preferences or a page covers the
  notes. So that an undoable delete is noticed even by a distracted
  user, the notice slides down and fades in over two `--duration`, then the Undo button pulses twice
  (an accent outline with the selected background and accent text, five `--duration` each); with
  reduced motion it appears at once and Undo keeps that selected look. Undo brings the note and its
  pinned items back and selects it. The selection otherwise moves to the next item. A note over the
  2 MB limit is not deleted: the same notice, with an alert icon and a close button, says it is too
  large to keep a copy for Undo and to delete it in File Explorer.
- **Focus:** clicking the panel or the shortcut gives it focus; it then stays open until focus moves to
  another app, Esc from the search bar, or the shortcut from the search bar. A pinned panel only
  closes by unpinning: Esc or the shortcut from its search bar returns focus to the previous app and it
  stays shown; the shortcut then takes focus back. Without focus (hover only), it closes when the cursor leaves. While the panel or its
  page has focus, a 2 px `--color-accent` band runs the full height of the inner edge, over the border and
  above the content, without moving anything.
- **Tab stops:** search bar → add note (+) → recovery actions when shown (Undo after a delete, "Restore" on
  leftovers, then "Restore pinned items") → list → note order button → top bar buttons (including
  Preferences) → resize handle → search bar, reversed with Shift+Tab. Card
  controls stay clickable and labeled but are outside the Tab sequence. The list is one Tab stop;
  entering it without a selection selects the first item. Its only focus indicator is the selected
  card's existing style, without an additional outline. The editor keeps Tab indentation. The list is a
  labeled region of plain groups, not a listbox, since cards hold buttons, links and the editor; the
  selected item has `aria-current` and a polite live region announces it.
- **Section jumps:** Ctrl+↑ / Ctrl+↓ select the first note of the previous / next visible section:
  Pinned, Notes and each folder or date group inside Notes. Folder labels are never destinations, and
  folders without notes are skipped; from a selected label, Ctrl+↓ goes to its first note. Notes and
  its first group share a destination when there is no root item before it.
  Jumps stay at the first / last section at the ends; with no selection they select the first section.
  Search and the editor keep their native Ctrl+arrow behavior.
- **Keyboard:** with focus anywhere in the panel except a text field or the editor, the list keys below
  work as from the search bar, and typing a character goes to the search bar. Enter acts on the
  selection even on a focused button; Space presses the button.
- **Details:** the "Create" item is the first item, under the bar; the first match is selected, so Enter
  expands it and Ctrl+Enter or ↑ reaches Create. Every way of creating opens the new note expanded and
  editing, so typing goes into it. Pinning uses a bookmark icon, distinct from the header's
  thumbtack that keeps the panel open. Rows and lines show a snippet with the matches highlighted under
  the title; previews and expanded cards show the content. Reading cards hide the frontmatter title
  (the header has it); the frontmatter title stays visible while editing. The expanded card lists every
  tag. → and ← act on the selected card only with the caret at the end of the text, otherwise they move it.

| Shortcut                  | Action                                                                                                 |
| ------------------------- | ------------------------------------------------------------------------------------------------------ |
| ↑ / ↓                     | Select a card, folder label or row; on a long expanded card, first scroll about three lines to its end |
| Alt+Shift+↑ / Alt+Shift+↓ | Move the selected pinned item up / down                                                                |
| Alt+↑ / Alt+↓             | Select the previous / next item directly                                                               |
| Ctrl+↑ / Ctrl+↓           | Jump to the previous / next section, only with focus on the list                                       |
| PgUp / PgDn               | Scroll the list by about one screen                                                                    |
| → / ←                     | Open / close one level: line, preview, expanded                                                        |
| Enter                     | Expand the selected item, run "Create", or remove a selected missing pinned item                       |
| Delete                    | Remove the selected missing pinned item                                                                |
| Ctrl+Delete               | Delete the selected note; outside text fields                                                          |
| Ctrl+M                    | Move the selected note to a folder; from the search bar or the list                                    |
| Shift+F10 / Menu          | Open the selected note or folder menu                                                                  |
| Ctrl+Enter                | Create a note with the exact typed text                                                                |
| Ctrl+D                    | Toggle pin on the selected card from search or the list; in the editor, see below                      |
| Ctrl+Shift+D              | Toggle the panel pin                                                                                   |
| Ctrl+,                    | Open / close Preferences                                                                               |
| Ctrl+/                    | Open / close the shortcut sheet                                                                        |
| Ctrl+E                    | Toggle editing on the selected card                                                                    |
| Ctrl+F                    | Focus the search bar and select its text; the editor finds in the note first                           |
| Esc                       | Leave editing; next, return to the bar and select its text; from the bar, close                        |
| Ctrl+Alt+Space (default)  | Open and go to the search bar with its text selected; from the search bar, close                       |

## Preferences

The top bar gear replaces the folder action and opens a third view with quiet General, Appearance, Browser,
Shortcuts and Trash section buttons, wrapping when the panel is narrow. The page stays loaded but hidden. The gear or Esc returns to the previous
Notes/Browser tab; either tab also leaves Preferences. The global shortcut returns to Notes and search.
General contains the folder path, folder picker, optional Vault file, note order, date grouping, relative dates, panel side and a
"Start with Windows" checkbox. Side and startup changes mirror the tray, including changes made
from the tray while Preferences is open. Startup is read from Windows, with a disabled control until
the initial read completes; failures appear in the existing notice and the actual state stays shown. Browser
shows DuckDuckGo as read-only and a compact Allowed sites list, with a neutral Remove button per
site. A short hint explains the subdomain scope.
The empty list explains how to add an exception using the shield. Shortcuts and the Ctrl+/ sheet share one list of panel/list and editor
keys, deriving formatting keys from the editor bindings; the activation row shows the current
combination. Shortcuts starts with the activation shortcut: the combination in a field-like box and a
neutral Change button. Change records: the box asks for a combination and shows the held modifiers, the
button becomes Cancel, the current global shortcut and panel shortcuts stand aside, and the next
non-modifier key ends recording; the key shown is the one the layout gives, such as A for the AZERTY
key at QWERTY's Q. Esc, Tab, leaving or hiding the panel, or 30 seconds without a key cancels. A refused combination (no Ctrl,
Alt or Win; Ctrl+Alt with a letter or digit, which is AltGr on layouts such as ABNT2; a key that is not a
letter, digit, F1–F24, arrow, Space or navigation key on the current layout) or one taken by another app keeps the previous shortcut and explains why in the
danger color below the hint. If the saved combination cannot be registered at startup, the default is
used and a dismissible warning notice under the top bar names both. From anywhere in Notes (including search and editing), Ctrl+/ opens
a quiet, scrollable in-webview dialog using the existing surface and line tokens, without a shadow
or dimming. Focus stays inside; Esc, Ctrl+/ again or the close button dismisses it and restores the
previous focus, including the editor cursor. No native popup takes panel focus. Appearance offers
System (default), Light and Dark theme radio buttons, applied immediately and remembered on this
computer; it also shows the panel width, how to resize it and a reset. Folder choice and width reset
use neutral buttons. The active section has an accessible name without repeating the tab
label as a heading. Sections scroll inside the panel, use the same insets and tokens, and have no new colors.
Opening a page preserves the Notes query.

Vault file shows its path (wrapping when necessary), or a short optional hint, with neutral Choose file
and Clear buttons. The native picker filters `.cryptomator` files. With a configured vault and a
notes folder that is not available, the list's empty state, the editor's folder-away notice (including
missing and unrecognized folders) and the folder setting show Unlock. In the list and Preferences it
sits beside the neutral Choose folder action. Unlock opens the saved file in its associated app; Cryptomator
handles authentication, including Windows Hello when configured. A missing vault file shows a nearby
error asking to choose it again. The existing folder poll restores the list without another action.

Trash explains in one muted paragraph that items the Recycle Bin cannot take stay there 30 days, inside
the notes folder (encrypted in a vault) and synced with it. Then one row per item, newest first,
between lines: the name (a note's title without `.md`) and, muted below it, its original folder
("Notes folder" for the root) and the relative deletion date, with the full date on hover; a neutral
Restore button on the right. Restoring says "Restored as path." below, since a taken name gives a free
one such as "name (2)". Under the list, the danger "Empty trash" button asks in the confirmation dialog
with the item count, confirmed by the danger "Delete for good": the only permanent action, so the
only question. Empty: "The trash is empty."; while the folder is away, a sentence that the trash
appears when it is back. An entry whose item cannot be told, as after a sync conflict, shows
"Unrecognized item" with its place in the trash and no Restore; Empty removes it. The trash is never
shown in the notes list.

## Panel navigation

The top bar shows subtle Notes and Browser tabs, followed in Browser by discreet back/forward
controls and the Reader view toggle (an open book, pressed while reading), then the Preferences gear (Ctrl+,) and panel pin (Ctrl+Shift+D). The panel is 460 logical pixels wide by default;
a 6-pixel handle along the inner edge resizes it (double-click restores the default).
The panel pin is filled when active, outlined otherwise, and 12% smaller than other icons.
Ctrl+Tab switches Notes/Browser while the main window has focus, including in the editor.
Button tooltips include their working shortcuts, including Ctrl+, on Preferences and Ctrl+Shift+D on the panel pin. There is no app name or page title. The same search
field and "+" action occupy the second bar in both modes. In Browser a quiet shield button sits
immediately left of the address field, sharing its row without taking space from the top navigation;
its icon, not its larger hit area, starts at the bar's content edge.
Shield means content blocking on; shield-off means allowed. One click toggles the whole registrable
site and reloads the page, without a popover. The tooltip and accessible label name the site and
current state. It is hidden without an HTTP(S) page and disabled while saving. Browser starts with a short hint and the
field focused; switching tabs retains the page document and history. The native page is clipped two
logical pixels inside the panel sides and bottom so the focus band remains visible.

Reader view replaces the page, under the same bars, with its article as a note reads: the title as
the note header, then text, headings, lists, quotes, code and tables in the reading styles, with the
panel's insets and its own scroll. Images are left out. The toggle is disabled while the article is
read; a failure says so under the top bar ("No article text was found on this page."). Links open in
the page as in notes. The bar keeps the page URL, and "+" saves the whole article. Pressing the
toggle again, back/forward, opening a page, switching tabs or Preferences return to the page.

Tag filters in Notes use whitespace-delimited `#tag` tokens. All tags must match a complete
frontmatter tag, ignoring case and accents, alongside the existing free-text search. Inline body
hashtags are ordinary text. A lone `#`, headings like `# Title`, and hashes inside words or URLs stay
literal. Tags containing spaces or quotes use a JSON-quoted token, such as `#"two words"`.
While the caret is in a tag token, the folder's matching tags appear below the bar in the existing
suggestion style. Up/Down select, Enter or Tab inserts the whole token and a space, and clicking also
accepts. Esc dismisses completion first, then follows the existing focus/close layers. Selecting the
bar's text and typing replaces the whole query. Browser and `?` modes keep their web suggestions.
Queries containing filters (or beginning with `#`) never offer automatic Create. Explicit Ctrl+Enter
and "+" create from the free text only; a tags-only query creates an untitled note. Creation seeds
all tag filters in frontmatter so the note remains in the results; date filters are ignored for
creation, and the new note is dated now. Typing in such a note starts after its frontmatter. The web
search row offers only the free text, and none for a filters-only query.
While completion is open, its partial tag token is ignored for filtering; accepting it applies the
complete tag. A sole suggestion identical to the complete token leaves list navigation available.
Keyboard caret movement updates completion; leaving the token dismisses it.

Date filters share the tag completion list: `date:` offers Today (`date:today`), Last 7 days
(`date:7d`, today and six preceding local days), This month (`date:month`) and Last month
(`date:last-month`). Labels are localized; tokens are language-neutral. Typed years, months and days
(`date:2026`, `date:2026-10`, `date:2026-10-03`) and inclusive ranges
(`date:2026-10..2026-11`, `date:2026-10..`, `date:..2026`) work without a calendar or chips.
Invalid or reversed date tokens are ignored and excluded from free text and note names.
All valid date filters, tag filters and free-text words apply together.

## Unified address bar

In browser mode the bar shows the current page URL; typing offers an Open/Search action, matching
local addresses, then engine suggestions. The same arrows, Enter and selected row style apply.
When the suggestion list is closed, ↓ opens the five most recent local addresses (including
search result URLs), even with an empty bar or the current URL. This does not request engine
suggestions; typing resumes the normal suggestion rules.
Suggestions float below the bar on one solid surface, with icons for search, history and Open;
only the selected row is highlighted. The native page is clipped behind them without moving or resizing. Esc restores
the current URL and dismisses suggestions before the next Esc closes the panel.

In notes mode the search persists until the user replaces or deletes its text; Esc, note creation,
opening a page, hiding the panel and focus loss never clear it. Returning to the bar with Esc or the
activation shortcut selects its text so typing replaces the previous search.

In notes mode Enter always activates the selected entry. URL-like text offers an explicit Open entry
after notes; only an explicit http(s) prefix selects it by default. Otherwise matches, then Create,
keep priority, including names such as `todo.md` and `1.5`. Other text offers Search the web; choosing
it enters `?` mode and enables remote suggestions, as does typing `?`. Ordinary note queries remain
local. Web actions use neutral icon rows; all untrusted suggestions appear as text. In Notes they
follow the notes as a last "Search the web" section, with note row geometry and selection.

## Note editor

The editor lives in the card being edited, in place of its content; there is no separate note view and
no separate editor toolbar. The card keeps its header and its button order: the pencil, left of pin, stays
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
| Ctrl+/                                        | Open / close the shortcut sheet                                                            |
| Ctrl+;                                        | Add a frontmatter property: a new line in the block, created if needed, with keys offered  |
| Ctrl+E                                        | Toggle reading and editing from anywhere in the panel                                      |
| Ctrl+F / Ctrl+H                               | Find / replace in the note, starting from the selection; Ctrl+F again goes to the bar      |
| Enter / Shift+Enter, F3 / Shift+F3            | Next / previous match                                                                      |
| Ctrl+D                                        | Select the next occurrence of the selection, adding a cursor                               |
| Alt+click                                     | Add a cursor                                                                               |
| Esc                                           | Close the find panel, then leave editing                                                   |

Find and replace is one compact row stuck to the top of the editing card: a replace toggle, the
field, a muted "3 of 12" count, match case (Aa) and regular expression (.*) toggles, previous, next
and close. The replace row, aligned with the field, shows only after Ctrl+H or the toggle. Matches
look like search highlights in the list, and the current one is outlined in the accent color.

Frontmatter stays YAML text, with no property form. Inside the raw block, typing at the start of a
line offers common keys not yet present (title, tags, aliases, date, author, description, status,
url), and typing a tag offers the folder's tags, in a quiet monospace list with the selected
background. Esc first closes the list. Unreadable tags have a muted wavy underline on their raw
frontmatter line and an explanatory tooltip. Leaving editing repairs comma-separated plain words
into a list and quotes plain `word: word` items inside a flow list, as one undoable edit saved by
autosave. Typing never triggers repair. Ambiguous YAML (including a scalar `tags: Plano: Q3`,
duplicate tag keys, incomplete lists, structured values and unreadable block items) stays untouched.

Inline formatting wraps the selection, unwraps matching markers inside or included in the selection,
and inserts a pair around an empty cursor. Block commands touch only selected line prefixes. Every
formatting command is one undo step. Shared checkbox styling and code-block framing keep the same
appearance in reading and editing.

Typing `(`, `[`, `{`, single or double quotes inserts the closing character; typing that closer
steps over it. Quotes after word characters stay literal, as in `don't`. Backspace removes an empty
pair before trying list/quote markup removal. Backticks remain literal for Markdown code fences;
the inline-code shortcut still wraps a selection or inserts a pair.

A compact formatting bar floats above a single non-empty selection, appearing after mouse release
and hiding during selection dragging, on blur, or while the editor is read-only (including image
paste). Its icon buttons toggle bold, italic, strike, inline code, heading level 1, bullet list,
task list and quote, insert a link, or add a frontmatter property. Labels and shortcuts in tooltips
come from the command bindings, in the interface language. The bar uses the surface, line and control
tokens without a shadow. Clicking preserves editor focus and selection; buttons stay outside the Tab
sequence, with existing shortcuts as the keyboard path. Esc keeps completion and find-panel priority.

Copy and cut in the panel (editor, reading views, search bar and other fields) put plain text on the
clipboard, kept out of clipboard history (Win+V) and cloud clipboard, and never cleared after a delay.
Pasting elsewhere keeps the text, not formatting or images. The panel's context menu has no Copy link
or Copy image. The web page copies as any browser does.
