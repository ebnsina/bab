import AppKit

/// A real AppKit application: a real menu bar, real traffic lights, real window chrome.
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var window: NSWindow!

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMenu()

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 940, height: 600),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "bab"
        window.center()
        window.setFrameAutosaveName("bab")
        window.minSize = NSSize(width: 400, height: 240)

        // The grid runs edge to edge behind a transparent titlebar, so the traffic
        // lights float over the terminal rather than sitting in a separate strip.
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isMovableByWindowBackground = true
        window.isOpaque = false
        window.backgroundColor = .clear

        // Vibrancy behind a translucent grid. This is the whole reason the core
        // clears to an alpha below one.
        let vibrancy = NSVisualEffectView(frame: window.contentLayoutRect)
        vibrancy.autoresizingMask = [.width, .height]
        vibrancy.material = .underWindowBackground
        vibrancy.blendingMode = .behindWindow
        vibrancy.state = .active

        let view = TerminalView(frame: vibrancy.bounds)
        view.autoresizingMask = [.width, .height]
        vibrancy.addSubview(view)

        window.contentView = vibrancy
        window.makeFirstResponder(view)
        window.makeKeyAndOrderFront(nil)

        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    /// Without a menu, cmd-Q does not quit and the app cannot be left.
    private func buildMenu() {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "About bab", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit bab", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        mainMenu.addItem(appItem)

        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Paste", action: nil, keyEquivalent: "v")
        editItem.submenu = editMenu
        mainMenu.addItem(editItem)

        NSApp.mainMenu = mainMenu
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = AppDelegate()
app.delegate = delegate
app.run()
