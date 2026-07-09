//! DEC private modes (`CSI ? Pm h` / `CSI ? Pm l`) and cursor style.

/// How the cursor is drawn, set by `DECSCUSR` (`CSI Ps SP q`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CursorShape {
    Block,
    Underline,
    Bar,
}

/// Cursor appearance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CursorStyle {
    pub shape: CursorShape,
    pub blink: bool,
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self {
            shape: CursorShape::Block,
            blink: true,
        }
    }
}

impl CursorStyle {
    /// Map a `DECSCUSR` parameter. `0` means "reset to default", as does an omitted one.
    #[must_use]
    pub const fn from_decscusr(param: u16) -> Option<Self> {
        Some(match param {
            0 | 1 => Self {
                shape: CursorShape::Block,
                blink: true,
            },
            2 => Self {
                shape: CursorShape::Block,
                blink: false,
            },
            3 => Self {
                shape: CursorShape::Underline,
                blink: true,
            },
            4 => Self {
                shape: CursorShape::Underline,
                blink: false,
            },
            5 => Self {
                shape: CursorShape::Bar,
                blink: true,
            },
            6 => Self {
                shape: CursorShape::Bar,
                blink: false,
            },
            _ => return None,
        })
    }
}

/// Mouse reporting granularity.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MouseTracking {
    #[default]
    Off,
    /// `?1000` — press and release only.
    Click,
    /// `?1002` — plus motion while a button is held.
    Drag,
    /// `?1003` — all motion.
    Motion,
}

/// Terminal modes that outlive a single escape sequence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Modes {
    /// `?1` `DECCKM`. Arrow keys send `SS3` instead of `CSI`.
    pub application_cursor_keys: bool,
    /// `?7` `DECAWM`. Wrap at the right margin.
    pub autowrap: bool,
    pub cursor_style: CursorStyle,
    /// `?25` `DECTCEM`. Cursor visibility.
    pub cursor_visible: bool,
    /// `?1049`. The alternate screen is active.
    pub alt_screen: bool,
    /// `?2004`. Wrap pasted text in `ESC [ 200 ~` / `ESC [ 201 ~`.
    pub bracketed_paste: bool,
    /// `?2026`. Hold rendering until the batch ends, killing flicker in TUIs.
    pub synchronized_output: bool,
    /// `?2031`. The application wants unsolicited color-scheme change notifications.
    pub color_scheme_updates: bool,
    /// `?1006` SGR mouse encoding.
    pub sgr_mouse: bool,
    pub mouse_tracking: MouseTracking,
}

impl Default for Modes {
    fn default() -> Self {
        Self {
            application_cursor_keys: false,
            cursor_style: CursorStyle::default(),
            autowrap: true,
            cursor_visible: true,
            alt_screen: false,
            bracketed_paste: false,
            synchronized_output: false,
            color_scheme_updates: false,
            sgr_mouse: false,
            mouse_tracking: MouseTracking::Off,
        }
    }
}

/// A DEC private mode we understand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    ApplicationCursorKeys,
    Autowrap,
    CursorVisible,
    AltScreen,
    BracketedPaste,
    SynchronizedOutput,
    ColorSchemeUpdates,
    SgrMouse,
    MouseTracking(MouseTracking),
}

impl Mode {
    /// Map a DEC private mode number, or `None` if unsupported.
    #[must_use]
    pub const fn from_number(number: u16) -> Option<Self> {
        Some(match number {
            1 => Self::ApplicationCursorKeys,
            7 => Self::Autowrap,
            25 => Self::CursorVisible,
            1000 => Self::MouseTracking(MouseTracking::Click),
            1002 => Self::MouseTracking(MouseTracking::Drag),
            1003 => Self::MouseTracking(MouseTracking::Motion),
            1006 => Self::SgrMouse,
            // `?47` and `?1047` switch screens without saving the cursor; `?1049`
            // does both. We treat them alike and always save, which xterm permits.
            47 | 1047 | 1049 => Self::AltScreen,
            2004 => Self::BracketedPaste,
            2026 => Self::SynchronizedOutput,
            2031 => Self::ColorSchemeUpdates,
            _ => return None,
        })
    }
}
