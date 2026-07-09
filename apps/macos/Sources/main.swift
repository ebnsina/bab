import AppKit

/// A real AppKit application: a real menu bar, real traffic lights, real window chrome,
/// and the system's own tabs.
final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var windows: [TerminalWindow] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMenu()
        openWindow()
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    /// Cmd-N. A new window rather than a new tab, which is what the shortcut means
    /// everywhere else on the system.
    @objc func newWindow(_ sender: Any?) {
        openWindow()
    }

    private func openWindow() {
        let window = TerminalWindow()
        window.center()
        window.makeKeyAndOrderFront(nil)
        adopt(window)
    }

    /// Hold a window so ARC does not free it the moment it leaves scope, and let it go
    /// when it closes — otherwise the last window never releases and the app never
    /// quits, and the shell behind a closed tab keeps running.
    func adopt(_ window: TerminalWindow) {
        window.delegate = self
        windows.append(window)
    }

    func windowWillClose(_ notification: Notification) {
        guard let closing = notification.object as? TerminalWindow else { return }
        windows.removeAll { $0 === closing }
    }

    /// Without a menu, Cmd-Q does not quit and the app cannot be left.
    private func buildMenu() {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(
            withTitle: "About bab",
            action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
            keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(
            withTitle: "Quit bab", action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q")
        appItem.submenu = appMenu
        mainMenu.addItem(appItem)

        let shellItem = NSMenuItem()
        let shellMenu = NSMenu(title: "Shell")
        shellMenu.addItem(
            withTitle: "New Window", action: #selector(newWindow(_:)), keyEquivalent: "n")
        // `newWindowForTab:` is AppKit's own selector. It walks the responder chain to
        // the key window, which adds the new tab to its own group.
        shellMenu.addItem(
            withTitle: "New Tab",
            action: #selector(NSWindow.newWindowForTab(_:)), keyEquivalent: "t")
        shellMenu.addItem(
            withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        shellItem.submenu = shellMenu
        mainMenu.addItem(shellItem)

        // These items carry no action: `performKeyEquivalent` on the terminal view
        // handles the chords. They exist so the shortcuts are discoverable.
        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Copy", action: nil, keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: nil, keyEquivalent: "v")
        editItem.submenu = editMenu
        mainMenu.addItem(editItem)

        // AppKit fills this with Show Previous Tab, Move Tab to New Window, and the
        // rest — but only if a menu named "Window" exists for it to populate.
        let windowItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowItem.submenu = windowMenu
        mainMenu.addItem(windowItem)
        NSApp.windowsMenu = windowMenu

        NSApp.mainMenu = mainMenu
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = AppDelegate()
app.delegate = delegate
app.run()
