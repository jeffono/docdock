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

- Brazilian Portuguese, short and direct sentences.
- Buttons with infinitive verbs: "Open", "Choose folder".
- No exclamation marks, technical jargon or blaming the user.

## Rendered content

The note uses only these tokens, in `:global` selectors inside `.note`. CSP forbids inline styles: no
component depends on a `style` attribute.
