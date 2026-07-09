//! The C ABI. One header serves AppKit, GTK4, and WinUI3.
//!
//! Two rules hold everywhere below. A panic unwinding across the FFI boundary is
//! undefined behaviour, so every entry point catches. And every pointer from the host
//! is untrusted, so every entry point null-checks before dereferencing.

use std::ffi::{CStr, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use bab_input::{Key, Modifiers};

use crate::terminal::Terminal;

/// An opaque handle to a terminal.
#[repr(C)]
#[derive(Debug)]
pub struct BabTerminal {
    _private: [u8; 0],
}

/// Named keys. `BAB_KEY_CHAR` means "use the text argument instead".
///
/// Function keys are `BAB_KEY_F1 + n - 1`, so `F5` is 105.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BabKey {
    Char = 0,
    Enter = 1,
    Tab = 2,
    Backspace = 3,
    Escape = 4,
    Up = 5,
    Down = 6,
    Right = 7,
    Left = 8,
    Home = 9,
    End = 10,
    PageUp = 11,
    PageDown = 12,
    Insert = 13,
    Delete = 14,
    F1 = 101,
}

/// Modifier bits, matching `bab_input::Modifiers`.
pub const BAB_MOD_SHIFT: u32 = 1;
pub const BAB_MOD_ALT: u32 = 2;
pub const BAB_MOD_CONTROL: u32 = 4;
pub const BAB_MOD_SUPER: u32 = 8;

fn named_key(code: u32) -> Option<Key> {
    Some(match code {
        0 => return None,
        1 => Key::Enter,
        2 => Key::Tab,
        3 => Key::Backspace,
        4 => Key::Escape,
        5 => Key::Up,
        6 => Key::Down,
        7 => Key::Right,
        8 => Key::Left,
        9 => Key::Home,
        10 => Key::End,
        11 => Key::PageUp,
        12 => Key::PageDown,
        13 => Key::Insert,
        14 => Key::Delete,
        101..=112 => Key::Function((code - 100) as u8),
        _ => return None,
    })
}

/// Run `body`, swallowing panics so none unwinds into the host.
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| {
        eprintln!("bab: caught a panic at the FFI boundary");
        fallback
    })
}

/// Borrow a terminal from a host pointer, or do nothing if it is null.
unsafe fn with<T>(
    handle: *mut BabTerminal,
    fallback: T,
    body: impl FnOnce(&mut Terminal) -> T,
) -> T {
    if handle.is_null() {
        return fallback;
    }
    // SAFETY: the caller guarantees the handle came from `bab_terminal_new` and has
    // not been freed. The cast is sound because that is what the handle points to.
    let terminal = unsafe { &mut *handle.cast::<Terminal>() };
    guard(fallback, || body(terminal))
}

/// Create a terminal drawing into a `CAMetalLayer`, and spawn the user's shell.
///
/// Returns null on failure. The size is in physical pixels, not points, and `scale` is
/// the display's backing scale factor, which the font size is multiplied by.
///
/// # Safety
///
/// `layer` must be a valid `CAMetalLayer` that outlives the returned terminal. Must be
/// called on the main thread.
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_new(
    layer: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut BabTerminal {
    guard(ptr::null_mut(), || {
        if layer.is_null() || width == 0 || height == 0 {
            return ptr::null_mut();
        }
        // SAFETY: forwarded from the caller.
        match unsafe { Terminal::new_for_metal_layer(layer, width, height, scale) } {
            Ok(terminal) => Box::into_raw(Box::new(terminal)).cast(),
            Err(error) => {
                eprintln!("bab: failed to create terminal: {error:#}");
                ptr::null_mut()
            }
        }
    })
}

/// Destroy a terminal. Passing null is allowed and does nothing.
///
/// # Safety
///
/// `handle` must have come from `bab_terminal_new` and must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_free(handle: *mut BabTerminal) {
    if handle.is_null() {
        return;
    }
    guard((), || {
        // SAFETY: the caller guarantees the handle is live and was allocated by us.
        drop(unsafe { Box::from_raw(handle.cast::<Terminal>()) });
    });
}

/// Apply pending shell output and draw one frame.
///
/// Returns `false` once the shell has exited, which is the host's cue to close.
///
/// # Safety
///
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_frame(handle: *mut BabTerminal) -> bool {
    unsafe {
        with(handle, false, |terminal| match terminal.frame() {
            Ok(alive) => alive,
            Err(error) => {
                eprintln!("bab: frame failed: {error:#}");
                true
            }
        })
    }
}

/// Resize to a new physical pixel size, at the display's backing scale factor.
///
/// # Safety
///
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_resize(
    handle: *mut BabTerminal,
    width: u32,
    height: u32,
    scale: f32,
) {
    unsafe {
        with(handle, (), |terminal| {
            if let Err(error) = terminal.resize(width, height, scale) {
                eprintln!("bab: resize failed: {error:#}");
            }
        });
    }
}

/// Tell the terminal whether its window has keyboard focus.
///
/// # Safety
///
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_set_focused(handle: *mut BabTerminal, focused: bool) {
    unsafe { with(handle, (), |terminal| terminal.set_focused(focused)) }
}

/// Send a key press.
///
/// `key` is a `BabKey`. When it is `BAB_KEY_CHAR`, `text` supplies the characters the
/// platform's input method produced. `text` may be null, meaning empty.
///
/// # Safety
///
/// `handle` must be live, and `text` must be null or a valid NUL-terminated UTF-8
/// string that stays valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_key(
    handle: *mut BabTerminal,
    key: u32,
    text: *const c_char,
    modifiers: u32,
) {
    // Invalid UTF-8 from the host is dropped rather than guessed at.
    let text = if text.is_null() {
        String::new()
    } else {
        // SAFETY: the caller guarantees a NUL-terminated string.
        match unsafe { CStr::from_ptr(text) }.to_str() {
            Ok(text) => text.to_owned(),
            Err(_) => return,
        }
    };

    unsafe {
        with(handle, (), |terminal| {
            let modifiers = Modifiers::from_bits((modifiers & 0xff) as u8);
            if let Err(error) = terminal.key(named_key(key), &text, modifiers) {
                eprintln!("bab: key failed: {error:#}");
            }
        });
    }
}

/// The title the running application set, as a NUL-terminated UTF-8 string.
///
/// The pointer is owned by the terminal and stays valid until the next call to this
/// function on the same handle. Copy it if you need to keep it. Returns null if the
/// handle is null.
///
/// # Safety
///
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_title(handle: *mut BabTerminal) -> *const c_char {
    unsafe { with(handle, ptr::null(), |terminal| terminal.title().as_ptr()) }
}

/// Paste text, bracketed when the running application asked for it.
///
/// # Safety
///
/// `handle` must be live, and `text` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bab_terminal_paste(handle: *mut BabTerminal, text: *const c_char) {
    if text.is_null() {
        return;
    }
    // SAFETY: the caller guarantees a NUL-terminated string.
    let Ok(text) = (unsafe { CStr::from_ptr(text) }).to_str() else {
        return;
    };
    let text = text.to_owned();

    unsafe {
        with(handle, (), |terminal| {
            if let Err(error) = terminal.paste(&text) {
                eprintln!("bab: paste failed: {error:#}");
            }
        });
    }
}
