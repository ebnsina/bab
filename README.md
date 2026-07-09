# bab

A GPU-accelerated terminal emulator with native UI on every platform, built around one goal:
**render complex scripts correctly.**

No mainstream terminal renders Bengali, Devanagari, or Malayalam properly today. The blocker is not
fonts or GPUs — it is that `wcwidth()` counts codepoints while these scripts render clusters. `bab`
resolves that by making `wcwidth` authoritative for layout and confining shaping to rendering, so the
terminal and the applications running inside it can never disagree about where the cursor is.

## The name

**bab** — Arabic **باب**: a door, a gate. By extension, a chapter of a book.

A terminal is the door to the machine. It is also, for most of the world, a door that opens onto a
language other than your own.

The word is a palindrome. It reads the same left-to-right and right-to-left — which is the joke, and
also the point: `bab` cannot yet render باب correctly. Arabic needs bidirectional layout, and that is
an explicit non-goal for the first release. Bengali comes first because complex-script shaping and
bidi are separate problems, and solving them together solves neither.

The name is a promise the project has not kept yet.

## Status

Early. The pipeline runs end to end — a real shell on a pseudoterminal, parsed into a cluster-aware
grid, shaped with HarfBuzz, drawn on the GPU, in a native AppKit window.

**Bengali does not render correctly yet.** Shaping is right in isolation: conjuncts, reph, and
pre-base matras all pass their tests, and the grid agrees with the system `wcwidth` codepoint for
codepoint. But a real shell still drives the line out of alignment, and the cause is not yet known.
Chasing it means capturing the raw pty byte stream and diffing what the shell believes it wrote
against what the grid holds — measurement, not argument.

## Layout

```
crates/bab-vt      grid · cells · VT parser        done
crates/bab-pty     pseudoterminal · session        done
crates/bab-text    shaping · font fallback         done
crates/bab-render  wgpu renderer · glyph atlas     done
crates/bab-input   keyboard · mouse encoding       done
crates/bab-config  user configuration              done
crates/libbab      C ABI over the core             done
apps/macos         AppKit shell                    done
crates/bab-theme   theme import · OS-follow
crates/bab-ssh     ssh client and host manager
crates/bab-vfs     local and remote file browsing
apps/linux         GTK4 shell
apps/windows       WinUI3 shell
```

## Keys

| | |
|---|---|
| `Cmd-T` | new tab |
| `Cmd-N` | new window |
| `Cmd-W` | close |
| `Cmd-C` / `Cmd-V` | copy · paste |
| drag, double-click, triple-click | select cells · word · line |
| shift-drag | select while an application owns the mouse |
| scroll | scrollback, or arrow keys on the alternate screen |

Tabs are macOS's own, so tab dragging, the tab overview, and `Cmd-Shift-[` / `]` all work.

## Configure

Copy `bab.example.toml` to `~/.config/bab/bab.toml`, or point `$BAB_CONFIG` at it. Every key is
optional. A file that will not parse is reported and ignored rather than fatal — a terminal that
refuses to open because of a typo is a terminal you cannot fix the typo in.

## Build

```sh
cargo test                    # the core, headless
./apps/macos/build.sh         # the macOS app
open target/debug/bab.app
```

Run `target/debug/bab.app/Contents/MacOS/bab` directly to see stderr.

The core is a plain Rust library behind a flat C ABI (`crates/libbab/include/bab.h`). One header
serves AppKit, GTK4, and WinUI3, so the terminal is written once and the window three times.

## Fonts

`bab` bundles its fallback chain rather than trusting system font fallback, which resolves
differently on every machine. The default grid font is JetBrains Mono Nerd Font Mono: programming
ligatures, a tall x-height that reads well at small sizes, and the Nerd Font icon range that prompts
rely on.

It has no Bengali glyphs at all. Noto Sans Bengali sits behind it in the chain, which is why the
chain is a component and not a convenience.

| Font | Copyright | License |
|---|---|---|
| [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono), patched by [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) | 2020 The JetBrains Mono Project Authors | SIL OFL 1.1 |
| [Noto Sans Bengali](https://github.com/notofonts/bengali) | 2022 The Noto Project Authors | SIL OFL 1.1 |

License texts live beside the fonts in `assets/fonts/`.

## License

Apache-2.0
