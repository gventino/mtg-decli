# mtg-deck-builder

A terminal (TUI) deck builder for **Magic: The Gathering — Commander/EDH**, written in Rust with [ratatui](https://ratatui.rs). Card data comes from the [magicthegathering.io](https://docs.magicthegathering.io/) API, and card images are rendered **directly in the terminal**.

```
┌ Search [/] ──────────┐┌ untitled — 87/100 ✓ ─┐┌ Card ────────────────┐
│ > t:creature c:g elf ││ Commander            ││   ┌────────────┐     │
├ Results (24) ────────┤│   ♔ Atraxa ●●●●      ││   │ card image │     │
│ ▸ Elvish Mystic (G)  ││ ▾ Ramp (12)          ││   │   (Kitty)  │     │
│   Llanowar Elves     ││   1 Cultivate        ││   └────────────┘     │
│   ...                ││ ▾ Creatures (24)     ││ Elvish Mystic  (G)   │
│                      ││   ...                ││ Creature — Elf Druid │
└──────────────────────┘└──────────────────────┘└──────────────────────┘
 [/]search [a]dd [C]ommander [x]remove [c]ategory [S]tats [w]rite [?]help
```

## Image rendering

Uses [`ratatui-image`](https://crates.io/crates/ratatui-image), which auto-detects your terminal's graphics protocol:

| Terminal | Quality |
|---|---|
| Ghostty, Kitty | Kitty graphics protocol (best) |
| iTerm2, WezTerm | iTerm2 inline images |
| xterm, mlterm | Sixel |
| anything else | Unicode halfblocks (works everywhere) |

Images are cached on disk (`~/Library/Caches/mtg-deck-builder` on macOS, `~/.cache/mtg-deck-builder` on Linux).

## Build & run

```sh
cargo run --release
```

Requires Rust 1.85+. No native dependencies.

## Usage

| Key | Action |
|---|---|
| `/` | Edit search, `Enter` runs it |
| `Tab` | Switch focus (Results ↔ Deck) |
| `j`/`k`, `↑`/`↓`, `g`/`G` | Navigate |
| `n` / `p` | Next / previous results page |
| `a` / `Enter` | Add selected card to deck |
| `C` | Set selected result as commander |
| `x` / `d` | Remove deck card (or commander) |
| `c` | Assign custom category (e.g. "Ramp", "Removal"); empty reverts to automatic |
| `Space` | Collapse/expand category |
| `S` | Deck stats (mana curve, color pips, type counts) |
| `w` | Save deck (JSON) |
| `E` | Export `.txt` (Moxfield/Archidekt compatible) |
| `L` / `D` / `R` | Load / new / rename deck |
| `?` | Help |
| `q` | Quit (autosaves) |

### Search filters

Bare words match the card name. Optional tokens:

- `t:creature` — type (`t:instant`, `t:legendary` …)
- `c:rg` — colors (AND); `c:r|g` for OR
- `id:wub` — color identity
- `o:"draw a card"` — oracle text
- `r:rare` — rarity
- `s:CMD` — set code
- `f:any` — drop the default `gameFormat=Commander` filter

Example: `t:creature c:g o:"add" elf`

## Commander validation

The deck panel shows a live ✓/✗ badge checking:

- exactly 100 cards (commander included)
- singleton (except basic lands)
- every card within the commander's color identity
- commander is a legendary creature (or says it "can be your commander")
- cards legal in Commander (when the API provides legality data)

## Storage

Decks are saved as JSON under the platform data dir
(`~/Library/Application Support/mtg-deck-builder/decks` on macOS,
`~/.local/share/mtg-deck-builder/decks` on Linux), exported `.txt` files land in the same folder.

## Known API quirks

- The API returns one record per printing; results are deduplicated by name, preferring printings with images.
- `orderBy=name` triggers HTTP 500 on the live API, so sorting happens client-side.
- Cards without a Gatherer `multiverseid` have no image; searches filter to `contains=imageUrl` by default (use `f:any`-style raw queries via the API if you need everything).
- Gatherer images are ~200×285 px — fine for terminal rendering.
