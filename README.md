# mtg-decli

A terminal (TUI) deck builder for **Magic: The Gathering — Commander/EDH**, written in Rust with [ratatui](https://ratatui.rs). Card data comes from **two selectable sources** — [Scryfall](https://scryfall.com/docs/api) (default) and [magicthegathering.io](https://docs.magicthegathering.io/) — and card images are rendered **directly in the terminal**.

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

Images are cached on disk (`~/Library/Caches/mtg-decli` on macOS, `~/.cache/mtg-decli` on Linux).

## Install

Grab a prebuilt binary from the [latest release](https://github.com/gventino/mtg-decli/releases/latest), or install with cargo:

```sh
cargo install --git https://github.com/gventino/mtg-decli
```

## Build & run

```sh
cargo run --release                    # uses configured source (default: Scryfall)
cargo run --release -- --source mtgapi # override for this run
```

Requires Rust 1.85+. No native dependencies.

## Card sources

| | Scryfall (default) | MTG API |
|---|---|---|
| Images | 488×680 (sharp) | 200×285 (Gatherer) |
| Search | full [Scryfall syntax](https://scryfall.com/docs/syntax) | name + filter tokens |
| Page size | 175 | 50 |

- Press **`o`** in the app to switch source (persisted to config, current search re-runs).
- `--source scryfall|mtgapi` overrides the config for one run.
- Config lives at the platform config dir (`~/Library/Application Support/mtg-decli/config.json` on macOS).
- Card data and images from Scryfall are used under the [Wizards Fan Content Policy](https://company.wizards.com/fancontentpolicy); full card scans are shown unmodified.

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
| `o` | Switch card source (Scryfall ⇄ MTG API) |
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
- `f:any` — drop the default Commander format filter; `f:modern` targets another format

Example: `t:creature c:g o:"add" elf`

With the **Scryfall** source the query is passed through to Scryfall's engine, so its entire [search syntax](https://scryfall.com/docs/syntax) works (`cmc<=2`, `is:commander`, `pow>=4`, `e:neo`, negations with `-` …). `format:commander` is appended automatically unless you constrain a format yourself or use `f:any`.

## Commander validation

The deck panel shows a live ✓/✗ badge checking:

- exactly 100 cards (commander included)
- singleton (except basic lands)
- every card within the commander's color identity
- commander is a legendary creature (or says it "can be your commander")
- cards legal in Commander (when the API provides legality data)

## Storage

Decks are saved as JSON under the platform data dir
(`~/Library/Application Support/mtg-decli/decks` on macOS,
`~/.local/share/mtg-decli/decks` on Linux), exported `.txt` files land in the same folder.

## Known API quirks

- **MTG API**: returns one record per printing → results deduplicated by name preferring printings with images; `orderBy=name` triggers HTTP 500 → client-side sort; cards without a Gatherer `multiverseid` have no image (searches filter to `contains=imageUrl`).
- **Scryfall**: `unique=cards` rollup is used; a search with zero matches answers HTTP 404 with an error object (treated as an empty result); double-faced cards show the front face image and both faces' rules text.
