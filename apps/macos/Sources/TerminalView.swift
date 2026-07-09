import AppKit
import QuartzCore

/// An NSView whose backing layer *is* a CAMetalLayer.
///
/// Overriding `makeBackingLayer` matters: if the root layer is already a CAMetalLayer,
/// the core renders straight into it. Otherwise a sublayer gets inserted and we lose
/// control of its scale, which is how a terminal ends up blurry on a Retina display.
final class TerminalView: NSView {
    private var terminal: OpaquePointer?
    private var displayTimer: Timer?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not supported")
    }

    override func makeBackingLayer() -> CALayer {
        let layer = CAMetalLayer()
        layer.pixelFormat = .bgra8Unorm
        // Drawing continues while the window is resized, rather than stretching the
        // last frame.
        layer.presentsWithTransaction = false
        layer.needsDisplayOnBoundsChange = true
        return layer
    }

    override var acceptsFirstResponder: Bool { true }
    override var isFlipped: Bool { true }

    private var metalLayer: CAMetalLayer? {
        layer as? CAMetalLayer
    }

    /// Physical pixels, which is what the core wants. Points would render at 1x on a
    /// Retina display and look soft.
    private var pixelSize: (UInt32, UInt32) {
        let scale = window?.backingScaleFactor ?? 1.0
        let size = bounds.size
        return (UInt32(max(size.width * scale, 1)), UInt32(max(size.height * scale, 1)))
    }

    // MARK: - Lifecycle

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil, terminal == nil, let layer = metalLayer else { return }

        updateLayerScale()
        let (width, height) = pixelSize
        terminal = bab_terminal_new(Unmanaged.passUnretained(layer).toOpaque(), width, height)

        if terminal == nil {
            presentFailureAndExit()
            return
        }

        // A 60 Hz tick is a placeholder. Rendering on input, not on a fixed tick, is
        // what keeps latency under one refresh; that work needs damage tracking first.
        displayTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
            self?.tick()
        }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        updateLayerScale()
        resizeCore()
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        resizeCore()
    }

    private func updateLayerScale() {
        guard let layer = metalLayer, let window else { return }
        layer.contentsScale = window.backingScaleFactor
        let (width, height) = pixelSize
        layer.drawableSize = CGSize(width: Int(width), height: Int(height))
    }

    private func resizeCore() {
        guard let terminal else { return }
        updateLayerScale()
        let (width, height) = pixelSize
        bab_terminal_resize(terminal, width, height)
    }

    private func tick() {
        guard let terminal else { return }
        if !bab_terminal_frame(terminal) {
            teardown()
            NSApp.terminate(nil)
        }
    }

    private func teardown() {
        displayTimer?.invalidate()
        displayTimer = nil
        if let terminal {
            bab_terminal_free(terminal)
        }
        terminal = nil
    }

    deinit {
        teardown()
    }

    private func presentFailureAndExit() {
        let alert = NSAlert()
        alert.messageText = "bab could not start"
        alert.informativeText = "No GPU adapter, or the shell failed to spawn."
        alert.runModal()
        NSApp.terminate(nil)
    }

    // MARK: - Focus

    override func becomeFirstResponder() -> Bool {
        if let terminal { bab_terminal_set_focused(terminal, true) }
        return super.becomeFirstResponder()
    }

    override func resignFirstResponder() -> Bool {
        if let terminal { bab_terminal_set_focused(terminal, false) }
        return super.resignFirstResponder()
    }

    // MARK: - Input

    override func keyDown(with event: NSEvent) {
        guard let terminal else { return }

        let modifiers = babModifiers(event.modifierFlags)
        let (key, text) = babKey(for: event)

        text.withCString { pointer in
            bab_terminal_key(terminal, key, pointer, modifiers)
        }
    }

    /// Cmd-V pastes; every other Cmd chord belongs to the menu, not the shell.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.modifierFlags.contains(.command),
              event.charactersIgnoringModifiers == "v",
              let terminal,
              let text = NSPasteboard.general.string(forType: .string)
        else { return false }

        text.withCString { bab_terminal_paste(terminal, $0) }
        return true
    }

    private func babModifiers(_ flags: NSEvent.ModifierFlags) -> UInt32 {
        var modifiers: UInt32 = 0
        if flags.contains(.shift) { modifiers |= UInt32(BAB_MOD_SHIFT) }
        if flags.contains(.option) { modifiers |= UInt32(BAB_MOD_ALT) }
        if flags.contains(.control) { modifiers |= UInt32(BAB_MOD_CONTROL) }
        if flags.contains(.command) { modifiers |= UInt32(BAB_MOD_SUPER) }
        return modifiers
    }

    /// Resolve an NSEvent to a named key, or to the text the input method produced.
    ///
    /// `characters` already has shift, dead keys, and the IME applied, so it is the
    /// right source for printable input. Function keys arrive there too, encoded in a
    /// private-use area, which is why they are matched before falling through.
    private func babKey(for event: NSEvent) -> (UInt32, String) {
        let characters = event.charactersIgnoringModifiers ?? ""

        if let scalar = characters.unicodeScalars.first {
            switch Int(scalar.value) {
            case NSUpArrowFunctionKey: return (UInt32(BAB_KEY_UP), "")
            case NSDownArrowFunctionKey: return (UInt32(BAB_KEY_DOWN), "")
            case NSRightArrowFunctionKey: return (UInt32(BAB_KEY_RIGHT), "")
            case NSLeftArrowFunctionKey: return (UInt32(BAB_KEY_LEFT), "")
            case NSHomeFunctionKey: return (UInt32(BAB_KEY_HOME), "")
            case NSEndFunctionKey: return (UInt32(BAB_KEY_END), "")
            case NSPageUpFunctionKey: return (UInt32(BAB_KEY_PAGE_UP), "")
            case NSPageDownFunctionKey: return (UInt32(BAB_KEY_PAGE_DOWN), "")
            case NSInsertFunctionKey: return (UInt32(BAB_KEY_INSERT), "")
            case NSDeleteFunctionKey: return (UInt32(BAB_KEY_DELETE), "")
            case NSF1FunctionKey...NSF12FunctionKey:
                let offset = Int(scalar.value) - NSF1FunctionKey
                return (UInt32(BAB_KEY_F1) + UInt32(offset), "")
            default:
                break
            }
        }

        switch event.keyCode {
        case 36, 76: return (UInt32(BAB_KEY_ENTER), "")
        case 48: return (UInt32(BAB_KEY_TAB), "")
        case 51: return (UInt32(BAB_KEY_BACKSPACE), "")
        case 53: return (UInt32(BAB_KEY_ESCAPE), "")
        default: break
        }

        // Control chords need the unshifted character; typing needs the composed one.
        let modifiers = event.modifierFlags
        let text = modifiers.contains(.control) ? characters : (event.characters ?? "")
        return (UInt32(BAB_KEY_CHAR), text)
    }
}
