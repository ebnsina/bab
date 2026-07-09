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

        // Vibrancy behind a translucent grid, running the full height of the window
        // so it shows through the transparent titlebar too. This is the whole reason
        // the core clears to an alpha below one.
        let vibrancy = NSVisualEffectView()
        vibrancy.material = .underWindowBackground
        vibrancy.blendingMode = .behindWindow
        vibrancy.state = .active
        window.contentView = vibrancy

        // The grid stops below the titlebar. With `fullSizeContentView` the content
        // rect covers the whole window, so text would otherwise render underneath the
        // traffic lights. `contentLayoutGuide` is where the titlebar ends.
        let view = TerminalView(frame: .zero)
        view.translatesAutoresizingMaskIntoConstraints = false
        vibrancy.addSubview(view)

        guard let safeArea = window.contentLayoutGuide as? NSLayoutGuide else {
            fatalError("contentLayoutGuide should be a layout guide on a titled window")
        }
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: safeArea.topAnchor),
            view.leadingAnchor.constraint(equalTo: vibrancy.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: vibrancy.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: vibrancy.bottomAnchor),
        ])

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
