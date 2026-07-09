# bab

A GPU-accelerated terminal emulator with native UI on every platform, built around one goal:
**render complex scripts correctly.**

No mainstream terminal renders Bengali, Devanagari, or Malayalam properly today. The blocker is not
fonts or GPUs — it is that `wcwidth()` counts codepoints while these scripts render clusters. `bab`
resolves that by making `wcwidth` authoritative for layout and confining shaping to rendering, so the
terminal and the applications running inside it can never disagree about where the cursor is.

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
