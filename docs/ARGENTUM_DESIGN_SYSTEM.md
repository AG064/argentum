# Design system

Reference for contributors changing the Slint interface. The source of truth is
`ui/workbench/theme.slint`. Use its tokens instead of literal values in `.slint`
files. The Android app uses the same interface at narrow width.

## Where tokens live

| File | Holds |
| --- | --- |
| `ui/workbench/theme.slint` | `Theme`: colors, type, radii, spacing, sizes, motion, breakpoints. `TrustTheme`: fixed colors for approval prompts |
| `ui/tokens.slint` | `AgTheme`: the palette and spacing read by the brand and icon components in `ui/components` |

Rust sets every `Theme` value from the saved appearance settings, so a theme
change needs no rebuild. The values below are the defaults of the Argentum theme.
The other themes (Obsidian, Graphite, Ember, Silver) override the colors.

## Colors

Surfaces, darkest to lightest:

| Token | Value |
| --- | --- |
| `sidebar` | `#070708` |
| `canvas` | `#0B0B0D` |
| `code-bg` | `#0E0E11` |
| `surface` | `#131316` |
| `raised` | `#1A1A1E` |
| `overlay` | `#1E1E23` |

Interaction and lines: `hover` `#FFFFFF0F`, `pressed` `#FFFFFF1A`, `border`
`#FFFFFF14`, `border-strong` `#FFFFFF2B`, `scrim` `#000000A6`, `shadow`
`#00000090`, `thumb` `#FFFFFF`.

Text: `text` `#EDEDEF`, `text-2` `#A6A6AD`, `text-3` `#75757D`, `silver`
`#C9CCD1`.

Accent: `accent` `#D2202E`, `accent-hover` `#E8323F`, `accent-text` `#FFFFFF`,
`accent-soft` `#D2202E24`, `accent-ink` `#F0646E`.

Status and diffs: `ok` `#8FB996`, `warn` `#D9B36A`, `danger` `#F0646E`,
`diff-add-bg` `#8FB9961A`, `diff-add-text` `#A9D0AF`, `diff-del-bg`
`#D2202E1F`, `diff-del-text` `#F0949B`.

Text contrast is enforced when settings change: primary and secondary text need
4.5:1 against their background, quiet text and the accent 3:1.

### Approval prompts

`TrustTheme` has its own fixed colors (`surface`, `border`, `text`, `text-2`,
`code-bg`, `approve`, `approve-text`, `reject`, `reject-text`, `scrim`, `mark`,
`shadow`). Themes, presets and the agent cannot change them.

## Type

Fonts default to Segoe UI Variable (`font-ui`) and Cascadia Mono (`font-mono`).
Sizes scale with `font-scale`.

| Token | Size |
| --- | --- |
| `fs-xs` | 11 px |
| `fs-sm` | 12.5 px |
| `fs-md` | 14 px |
| `fs-lg` | 16 px |
| `fs-xl` | 20 px |
| `fs-2xl` | 28 px |

Weights: `w-normal` 400, `w-medium` 500, `w-semibold` 600.

## Spacing and radii

Spacing scales with `density`.

- `s1` to `s6`: 4, 8, 12, 16, 24 and 32 px.
- `sp0` to `sp22`: the pixel value in the name, for these steps only: 0 to 14,
  16, 20 and 22.

Radii scale with `radius-scale`: `r-xs` 4 px, `r-sm` 6 px, `r-md` 9 px, `r-lg`
13 px, `r-xl` 18 px.

## Layout and sizes

| Token | Value |
| --- | --- |
| `sidebar-width` | 264 px |
| `panel-width` | 460 px |
| `chat-width` | 760 px |
| `narrow-below` | 768 px |
| `panel-dock-min` | 980 px |
| `hero-compact-below` | 420 px |
| `touch` | 48 px |
| `control`, `chip`, `row`, `header` | 32, 28, 34 and 48 px (times `density`); `touch`, `touch`, `touch` and 56 px when `coarse` |
| `icon-sm`, `icon-md`, `icon-lg` | 14, 16 and 18 px |

Below 768 px the window is narrow: the sidebar becomes a drawer and `coarse` is
on, so every control is at least 48 px. The side panel docks only at 980 px or
wider.

## Motion

`quick` 110 ms, `calm` 190 ms, `glide` 230 ms, `ripple` 250 ms. All are 0 when
`reduced-motion` is on. Controls work the moment they are drawn.

## Brand

The source image is `assets/brand/argentum-source.png`. `assets/brand/manifest.json`
lists its hash, the crop, and every derived PNG, `.ico` and `.icns` file. Use
`BrandMark` for the symbol and `BrandLockup` for the symbol with the name. Do not
redraw, tint, stretch or crop the mark, and do not stand in for it with text.

In the conversation UI the mark appears once, in the new-task empty state.

## Icons

Icons are the Lucide set (ISC license), copied to `ui/assets/icons/lucide`.
`ui/assets/icons/catalog.json` maps each of the 41 roles (for example `new-task`)
to an upstream icon, its file and a SHA-256. Components ask for a role, not a
file.

## Checks

Run these from the repository root before changing the interface:

```powershell
pwsh scripts/validate-design-system.ps1
pwsh scripts/validate-mobile-design.ps1
pwsh scripts/validate-icon-system.ps1
```

- `validate-design-system.ps1` checks the token files, that the interface uses
  tokens instead of raw spacing and breakpoints, and the brand assets.
- `validate-mobile-design.ps1` checks the 768 px and 48 px tokens and that controls
  grow to the touch target at narrow widths.
- `validate-icon-system.ps1` checks the icon catalog against the files and the
  one-mark rule above.

To render screenshots of every interface state, see
[Interface modules](UI_MODULES.md).
