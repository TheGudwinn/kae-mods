# Guddion: design system ("Ledger")

## Idea
A mod manager is a ledger: a long list of things you own and can switch on or off. So the interface is built like
one. Page titles are set large in a serif; everything under them is a plain table of rows separated by hairlines.
Boxes appear only around pictures (mod covers, hero portraits). Colour is almost absent: black paper, steel-grey
ink, one white action per screen. Status colour is reserved for facts (applied, update available, error).

## Type (all IBM Plex, one family, Cyrillic included)
| Role | Face | Size / line | Weight |
|---|---|---|---|
| Page title | IBM Plex Serif | 36 / 1.1 | 500 |
| Section title | IBM Plex Serif | 22 / 1.2 | 500 |
| Body, controls, table text | IBM Plex Sans | 14 / 1.5 | 400, 500 for names |
| Small label (column heads, hints) | IBM Plex Sans | 12 / 1.4 | 500, no capitals, no tracking |
| Numbers, paths, versions | IBM Plex Mono | 13 / 1.4 | 400, tabular |

Rules: line length of running text 60-70ch. No all-caps. No mono for words, only for things that are data.

## Colour
| Token | Hex | Use |
|---|---|---|
| `--bg` | `#0B0B0C` | window, page |
| `--shell-raise` | `#141416` | covers, inputs, menus |
| `--text` | `#E9E8E4` | main text (warm, not pure white) |
| `--muted` | `#9A9CA2` | secondary text (AA on `#0B0B0C`) |
| `--dim` | `#74767C` | disabled, hints only |
| `--edge` | `rgb(255 255 255 / .10)` | hairline rules |
| `--white` | `#F2F2F3` | the one primary action |
| `--ok` | `#8FD1A8` | applied, installed |
| `--star` | `#D9B45A` | warning |
| `--danger` | `#E58A8F` | error, destructive |

## Space and shape
Scale (px): 4, 8, 12, 16, 24, 40, 64. Row height 44. Page gutter 40 (24 below 900). Radius 6 for controls,
8 for covers, nothing else. No shadows. Borders are hairlines or absent.

## Motion
Only state changes: hover background 120 ms, page change 200 ms fade, dialogs 200 ms. Nothing moves on its own.

## Components
- Ledger row: grid of columns, 44 px high, hairline below, background on hover, applied row has a 2 px white bar.
- Tabs and filters: text with a 2 px underline on the active one, not boxes.
- Primary button: white fill, one per view. Secondary: hairline outline. Tertiary: text with underline on hover.
- Notice: 2 px left rule in the status colour and text, no box.
