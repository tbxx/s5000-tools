# Style guide

The app follows Massimo Vignelli's 1970s NYC subway signage: Helvetica,
black signs with white text, route bullets, square corners, hairline
rules, lots of white (black in dark mode). Colour is used only for meaning:
bullets, active state, meters and LEDs. Keep it dense and functional, with no
decoration and no textures.

Every token is a CSS custom property in `src/app.css`. Light is the default.
Colour schemes (Preferences) are token sets under `[data-theme="…"]`: light,
dark, studio, ivory, contrast. "System" sets no `data-theme` and follows
`prefers-color-scheme`. Any element can carry `data-theme`, which is how the
Preferences tiles preview a scheme. Use the tokens; never hard-code colours in
a component (meter faces and LEDs, which imitate hardware, are the exception).

## Tokens

| Token | Use |
| --- | --- |
| `--font` | Helvetica Neue, Helvetica, Arial. Bold (700) for headings, buttons and labels. Tabular figures are on. |
| `--mono` | Only for data that needs it (hex, paths when column alignment matters). |
| `--bg`, `--surface` | Page and panel background: white, or black in dark mode. |
| `--surface-2` | Hover fill, wells, empty meter tracks. |
| `--rule` | Strong hairline: button borders, a rule under a heading or above a footer. |
| `--rule-soft` | Separators inside a panel, list rows, empty knob tracks. |
| `--text`, `--muted` | Body text; secondary text (units, counts, hints). |
| `--sign-bg`, `--sign-text`, `--sign-muted`, `--sign-rule` | Signage bars: background, text, dimmed text, the white hairline. |
| `--accent`, `--accent-text` | Active state and focus (MTA blue). |
| `--select`, `--select-text`, `--select-soft` | Selected rows: focused, and unfocused (soft). |
| `--ok`, `--bad`, `--warn` | Green, red and yellow for status. |
| `--led` | A lit mode LED. |
| `--route` | The current panel's route colour, read by knobs and graphs. Set it on a panel: `style:--route="var(--mta-green)"`. |
| `--lcd-bg`, `--lcd-fg`, `--lcd-edge` | The sampler's screen. |
| `--field-bg`, `--field-rule` | Inputs and selects. |
| `--mta-red` `#EE352E`, `--mta-green` `#00933C`, `--mta-blue` `#0039A6`, `--mta-orange` `#FF6319`, `--mta-yellow` `#FCCC0A`, `--mta-purple` `#B933AD`, `--mta-lime` `#6CBE45`, `--mta-grey` `#A7A9AC`, `--mta-brown` `#996633` | Route colours. |

The old names (`--panel`, `--panel-2`, `--line`, `--title-bg`, `--key`, …)
still map onto these so unconverted views keep working. Don't use them in new
code.

Spacing: 4, 8, 12, 16 or 24 px. Headings sit flush on their content, and
panels are separated by space plus a `--rule-soft` hairline, not by boxes.
There are no `border-radius` values (except round bullets and LEDs) and no
shadows.

## Components (global classes)

**Signage bar**: `<h3 class="sign">Sampler memory</h3>`. A full-width black
bar with white bold 12px text in mixed case, as on the station signs. Use it
for every panel heading. You can put buttons or a bullet inside it (it is a
flex row). For a short label inside a row, use `<span class="title">`. The top
bar is a larger sign with a 1px white rule 3px below its top edge (see
`App.svelte`).

**Route bullet**: `<span class="bullet red">2</span>`. An 18px filled circle
with a white letter or number. The colours are `red green blue orange yellow
purple lime grey brown`; yellow gets black text. Add `small` for 14px. Use
bullets sparingly, as identifiers: one per view in the navigation (Transfers
1 blue, Front panel 2 red, Meters 3 green, Program 4 orange). Don't use them
as decoration.

**Rule**: `<hr class="rule">` (strong) or `<hr class="rule soft">`, or a
`border-top: 1px solid var(--rule-soft)` on the element itself.

**Button**: a plain `<button>`, square with a 1px `--rule` border and bold
text. Hover fills it with `--surface-2`, and a press inverts it (black with
white text). `class="primary"` makes a solid black sign (white in dark mode).
For a destructive confirm, set the background to `--bad`. For segmented
choices (tabs, 1×/1.5×/2×), place buttons side by side with `margin-left: -1px`
and invert the active one (`background: var(--text); color: var(--bg)`).

**Field**: a plain `<input>` or `<select>`, square with a 1px `--field-rule`
border that turns `--rule` on focus. Checkboxes use `accent-color:
var(--text)`.

**Readout**: `<span class="lcd">`, a square value box with a hairline border
and tabular figures. Despite the name, it is not an LCD imitation.

**Knob** (`Control.svelte`): a thin `--rule-soft` ring, a solid 3.5px value
arc in `--route`, a hairline pointer, and a square readout below.

**Lists** (for Transfers): rows 22–24px tall separated by `--rule-soft`
hairlines (or zebra rows in `--surface-2`). The selected row is
`--select`/`--select-text`. Group headings are small bold text with a count in
`--muted`.

## Program editor route colours

Pitch is blue, amp red, filter green, LFO purple, aux envelope orange, tuning
brown, zone lime, and other grey. Each panel heading has a 6px left edge in
its route colour, and its knobs and envelope graph use the same colour.
