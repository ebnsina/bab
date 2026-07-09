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

Early. The cluster-aware grid and VT state machine are working and tested.

## Layout

```
crates/bab-vt    grid · cells · VT parser        ← you are here
crates/bab-pty   pseudoterminal
crates/bab-text  shaping · fallback · atlas
crates/bab-render wgpu renderer
crates/bab-theme theme import · OS-follow
crates/bab-ssh   ssh client and host manager
crates/bab-vfs   local and remote file browsing
apps/            AppKit · GTK4 · WinUI3 shells
```

## Build

```sh
cargo test
```

## License

Apache-2.0
