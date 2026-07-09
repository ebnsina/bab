import AppKit

/// One window, one shell.
///
/// Tabs are macOS's own: giving every window the same `tabbingIdentifier` lets the
/// system group them, which buys the real tab bar, the tab overview, tab dragging
/// between windows, and Cmd-Shift-[ / ] — none of which a hand-drawn tab bar would
/// have, and all of which users already know.
final class TerminalWindow: NSWindow {
    private let terminalView = TerminalView(frame: .zero)

    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 940, height: 600),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )

        title = "bab"
        minSize = NSSize(width: 400, height: 240)
        tabbingIdentifier = "bab"
        tabbingMode = .preferred
        // Under ARC a window that releases itself on close is a use-after-free waiting
        // to happen. The delegate drops its reference instead.
        isReleasedWhenClosed = false

        // The grid runs edge to edge behind a transparent titlebar, so the traffic
        // lights float over the terminal rather than sitting in a separate strip.
        titlebarAppearsTransparent = true
        titleVisibility = .hidden
        isMovableByWindowBackground = true
        isOpaque = false
        backgroundColor = .clear

        // Vibrancy behind a translucent grid, running the full height of the window so
        // it shows through the transparent titlebar too. This is the whole reason the
        // core clears to an alpha below one.
        let vibrancy = NSVisualEffectView()
        vibrancy.material = .underWindowBackground
        vibrancy.blendingMode = .behindWindow
        vibrancy.state = .active
        contentView = vibrancy

        // The grid stops below the titlebar. With `fullSizeContentView` the content
        // rect covers the whole window, so text would otherwise render underneath the
        // traffic lights. `contentLayoutGuide` is where the titlebar ends.
        terminalView.translatesAutoresizingMaskIntoConstraints = false
        vibrancy.addSubview(terminalView)

        guard let safeArea = contentLayoutGuide as? NSLayoutGuide else {
            fatalError("a titled window should have a content layout guide")
        }
        NSLayoutConstraint.activate([
            terminalView.topAnchor.constraint(equalTo: safeArea.topAnchor),
            terminalView.leadingAnchor.constraint(equalTo: vibrancy.leadingAnchor),
            terminalView.trailingAnchor.constraint(equalTo: vibrancy.trailingAnchor),
            terminalView.bottomAnchor.constraint(equalTo: vibrancy.bottomAnchor),
        ])

        makeFirstResponder(terminalView)
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    /// Cmd-T. AppKit walks the responder chain looking for this, and adds the new
    /// window to this window's tab group.
    override func newWindowForTab(_ sender: Any?) {
        let window = TerminalWindow()
        addTabbedWindow(window, ordered: .above)
        window.makeKeyAndOrderFront(nil)

        // A tab this window opened is still the delegate's to hold and to release.
        (NSApp.delegate as? AppDelegate)?.adopt(window)
    }
}
