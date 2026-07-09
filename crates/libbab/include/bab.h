// C ABI over the bab terminal core.
//
// The core owns the terminal; the host owns the window. Every function tolerates a
// null handle, and no function unwinds a panic into the caller.

#ifndef BAB_H
#define BAB_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct BabTerminal BabTerminal;

// Named keys. BAB_KEY_CHAR means "the text argument carries the characters".
// Function keys are BAB_KEY_F1 + n - 1, so F5 is 105.
enum {
  BAB_KEY_CHAR = 0,
  BAB_KEY_ENTER = 1,
  BAB_KEY_TAB = 2,
  BAB_KEY_BACKSPACE = 3,
  BAB_KEY_ESCAPE = 4,
  BAB_KEY_UP = 5,
  BAB_KEY_DOWN = 6,
  BAB_KEY_RIGHT = 7,
  BAB_KEY_LEFT = 8,
  BAB_KEY_HOME = 9,
  BAB_KEY_END = 10,
  BAB_KEY_PAGE_UP = 11,
  BAB_KEY_PAGE_DOWN = 12,
  BAB_KEY_INSERT = 13,
  BAB_KEY_DELETE = 14,
  BAB_KEY_F1 = 101,
};

enum {
  BAB_MOD_SHIFT = 1,
  BAB_MOD_ALT = 2,
  BAB_MOD_CONTROL = 4,
  BAB_MOD_SUPER = 8,
};

// Create a terminal drawing into a CAMetalLayer and spawn the user's shell.
// Size is in physical pixels and `scale` is the display's backing scale factor; the
// font size is multiplied by it. Returns NULL on failure. Main thread only.
BabTerminal *bab_terminal_new(void *layer, uint32_t width, uint32_t height,
                              float scale);

// Destroy a terminal. NULL is allowed.
void bab_terminal_free(BabTerminal *handle);

// Apply pending shell output and draw one frame.
// Returns false once the shell has exited, which is the cue to close the window.
bool bab_terminal_frame(BabTerminal *handle);

// Resize to a new physical pixel size, at the display's backing scale factor.
void bab_terminal_resize(BabTerminal *handle, uint32_t width, uint32_t height,
                         float scale);

// Tell the terminal whether its window has keyboard focus.
void bab_terminal_set_focused(BabTerminal *handle, bool focused);

// Send a key press. `text` may be NULL. When `key` is BAB_KEY_CHAR, `text` supplies
// what the input method produced.
void bab_terminal_key(BabTerminal *handle, uint32_t key, const char *text,
                      uint32_t modifiers);

// Paste text, bracketed when the running application asked for it.
void bab_terminal_paste(BabTerminal *handle, const char *text);

#ifdef __cplusplus
}
#endif

#endif // BAB_H
