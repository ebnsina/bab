//! End-to-end: a real child process on a real pseudoterminal.

#![cfg(unix)]

use std::ffi::OsString;
use std::time::{Duration, Instant};

use bab_pty::{Command, Session, Size};

const TIMEOUT: Duration = Duration::from_secs(10);

fn sh(script: &str) -> Command {
    Command {
        program: Some(OsString::from("/bin/sh")),
        args: vec![OsString::from("-c"), OsString::from(script)],
        ..Command::default()
    }
}

/// The whole screen, since the tty echoes input and pushes output down a row.
fn screen_text(session: &Session) -> String {
    let grid = session.terminal().grid();
    (0..grid.rows())
        .map(|row| grid.row_text(row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Pump until `predicate` holds, or fail. Never loops forever.
fn pump_until(session: &mut Session, predicate: impl Fn(&Session) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        session
            .pump_timeout(Duration::from_millis(50))
            .expect("pump failed");
        if predicate(session) {
            return;
        }
        if session.is_closed() {
            // Drain whatever arrived alongside the close.
            session.pump().expect("pump failed");
            assert!(predicate(session), "child exited before the condition held");
            return;
        }
    }
    panic!("timed out waiting for the terminal to reach the expected state");
}

#[test]
fn child_output_reaches_the_grid() {
    let mut session = Session::spawn(sh("printf 'hello'"), Size::new(4, 20)).unwrap();
    pump_until(&mut session, |s| s.terminal().grid().row_text(0) == "hello");
}

/// The whole point, end to end: Bangla survives a real PTY round trip.
#[test]
fn bangla_survives_the_pty() {
    let mut session = Session::spawn(sh("printf 'বাংলা'"), Size::new(4, 20)).unwrap();
    pump_until(&mut session, |s| s.terminal().grid().row_text(0) == "বাংলা");

    let clusters: Vec<_> = session
        .terminal()
        .grid()
        .clusters(0)
        .map(|c| c.text().to_owned())
        .collect();
    assert_eq!(clusters.len(), 2, "expected two clusters, got {clusters:?}");
}

#[test]
fn conjunct_occupies_one_cell_span() {
    let mut session = Session::spawn(sh("printf 'ব্ল'"), Size::new(4, 20)).unwrap();
    pump_until(&mut session, |s| s.terminal().grid().row_text(0) == "ব্ল");

    let clusters: Vec<_> = session.terminal().grid().clusters(0).collect();
    assert_eq!(clusters.len(), 1);
}

#[test]
fn escape_sequences_are_interpreted() {
    let mut session = Session::spawn(sh("printf 'ab\\033[1;1Hx'"), Size::new(4, 20)).unwrap();
    pump_until(&mut session, |s| s.terminal().grid().row_text(0) == "xb");
}

#[test]
fn input_reaches_the_child() {
    let mut session = Session::spawn(
        sh("read line; printf \"got:%s\" \"$line\""),
        Size::new(4, 20),
    )
    .unwrap();
    session.send(b"ping\n").unwrap();
    pump_until(&mut session, |s| screen_text(s).contains("got:ping"));
}

/// The terminal must answer queries, or applications that ask block forever.
///
/// The child reads in raw mode because a `DSR` reply carries no newline, and a
/// canonical-mode read would never return. `cat -v` renders the reply printably.
#[test]
fn cursor_position_query_is_answered() {
    let script = "stty raw -echo; printf '\\033[6n'; cat -v";
    let mut session = Session::spawn(sh(script), Size::new(4, 20)).unwrap();
    pump_until(&mut session, |s| screen_text(s).contains("[1;1R"));
    session.kill().unwrap();
}

#[test]
fn session_closes_when_the_child_exits() {
    let mut session = Session::spawn(sh("exit 0"), Size::new(4, 20)).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while !session.is_closed() && Instant::now() < deadline {
        session.pump_timeout(Duration::from_millis(50)).unwrap();
    }
    assert!(session.is_closed());
}

#[test]
fn resize_is_reflected_in_the_grid() {
    let mut session = Session::spawn(sh("sleep 30"), Size::new(4, 20)).unwrap();
    session.resize(Size::new(10, 40)).unwrap();

    assert_eq!(session.terminal().grid().rows(), 10);
    assert_eq!(session.terminal().grid().cols(), 40);
    session.kill().unwrap();
}

/// Without bracketing, a pasted newline runs a command. With it, the shell sees a paste.
#[test]
fn paste_is_bracketed_only_when_requested() {
    let mut session = Session::spawn(sh("cat"), Size::new(4, 40)).unwrap();
    session.paste("plain").unwrap();
    pump_until(&mut session, |s| {
        s.terminal().grid().row_text(0).contains("plain")
    });
    assert!(!session.terminal().modes().bracketed_paste);
    session.kill().unwrap();
}

// ---- keyboard round trip ---------------------------------------------------

use bab_input::{Key, Modifiers, keyboard};

/// The loop closes: an encoded key press reaches the child and comes back as output.
#[test]
fn an_encoded_keypress_reaches_the_child() {
    let mut session = Session::spawn(sh("cat"), Size::new(4, 20)).unwrap();

    let modes = *session.terminal().modes();
    for c in ['h', 'i'] {
        let bytes = keyboard::encode(&Key::Char(c), Modifiers::NONE, &modes).unwrap();
        session.send(&bytes).unwrap();
    }
    let enter = keyboard::encode(&Key::Enter, Modifiers::NONE, &modes).unwrap();
    session.send(&enter).unwrap();

    pump_until(&mut session, |s| screen_text(s).contains("hi"));
    session.kill().unwrap();
}

/// ctrl-d is end of transmission, so `cat` sees EOF and exits. If the encoder emitted
/// a literal `d` instead, the session would never close.
#[test]
fn control_d_closes_a_reading_child() {
    let mut session = Session::spawn(sh("cat"), Size::new(4, 20)).unwrap();
    let modes = *session.terminal().modes();

    let bytes = keyboard::encode(&Key::Char('d'), Modifiers::CONTROL, &modes).unwrap();
    assert_eq!(bytes, vec![0x04]);
    session.send(&bytes).unwrap();

    let deadline = Instant::now() + TIMEOUT;
    while !session.is_closed() && Instant::now() < deadline {
        session.pump_timeout(Duration::from_millis(50)).unwrap();
    }
    assert!(session.is_closed(), "ctrl-d should have ended the child");
}
