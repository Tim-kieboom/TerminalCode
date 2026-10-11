use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
use vt100::{MouseProtocolEncoding as Encoding, MouseProtocolMode as Mode};

use crate::pty::encode_mouse;

const NONE: KeyModifiers = KeyModifiers::NONE;

fn sgr(kind: MouseEventKind, mode: Mode, column: u16, row: u16) -> Option<Vec<u8>> {
    encode_mouse(kind, NONE, column, row, mode, Encoding::Sgr)
}

#[test]
fn a_program_that_did_not_ask_gets_nothing() {
    assert_eq!(
        sgr(MouseEventKind::Down(MouseButton::Left), Mode::None, 0, 0),
        None
    );
}

#[test]
fn sgr_reports_presses_and_releases_with_one_based_positions() {
    let press = sgr(
        MouseEventKind::Down(MouseButton::Left),
        Mode::PressRelease,
        4,
        9,
    );
    let release = sgr(
        MouseEventKind::Up(MouseButton::Left),
        Mode::PressRelease,
        4,
        9,
    );

    assert_eq!(press.unwrap(), b"\x1b[<0;5;10M");
    assert_eq!(release.unwrap(), b"\x1b[<0;5;10m");
}

#[test]
fn the_other_buttons_have_their_own_numbers() {
    let middle = sgr(
        MouseEventKind::Down(MouseButton::Middle),
        Mode::PressRelease,
        0,
        0,
    );
    let right = sgr(
        MouseEventKind::Down(MouseButton::Right),
        Mode::PressRelease,
        0,
        0,
    );

    assert_eq!(middle.unwrap(), b"\x1b[<1;1;1M");
    assert_eq!(right.unwrap(), b"\x1b[<2;1;1M");
}

#[test]
fn the_wheel_is_reported_as_buttons_64_and_up() {
    assert_eq!(
        sgr(MouseEventKind::ScrollUp, Mode::PressRelease, 2, 3).unwrap(),
        b"\x1b[<64;3;4M"
    );
    assert_eq!(
        sgr(MouseEventKind::ScrollDown, Mode::PressRelease, 2, 3).unwrap(),
        b"\x1b[<65;3;4M"
    );
    assert_eq!(
        sgr(MouseEventKind::ScrollLeft, Mode::PressRelease, 0, 0).unwrap(),
        b"\x1b[<66;1;1M"
    );
}

#[test]
fn modifiers_are_added_to_the_button() {
    let report = encode_mouse(
        MouseEventKind::Down(MouseButton::Left),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
        0,
        0,
        Mode::PressRelease,
        Encoding::Sgr,
    );

    assert_eq!(report.unwrap(), b"\x1b[<24;1;1M");
}

#[test]
fn x10_mode_reports_presses_only_and_without_modifiers() {
    let press = encode_mouse(
        MouseEventKind::Down(MouseButton::Left),
        KeyModifiers::CONTROL,
        0,
        0,
        Mode::Press,
        Encoding::Sgr,
    );

    assert_eq!(press.unwrap(), b"\x1b[<0;1;1M");
    assert_eq!(
        sgr(MouseEventKind::Up(MouseButton::Left), Mode::Press, 0, 0),
        None
    );
    assert_eq!(
        sgr(MouseEventKind::Drag(MouseButton::Left), Mode::Press, 0, 0),
        None
    );
}

#[test]
fn motion_is_reported_only_as_far_as_the_mode_asked() {
    let drag = MouseEventKind::Drag(MouseButton::Left);

    assert_eq!(sgr(drag, Mode::PressRelease, 1, 1), None);
    assert_eq!(
        sgr(drag, Mode::ButtonMotion, 1, 1).unwrap(),
        b"\x1b[<32;2;2M"
    );
    assert_eq!(sgr(MouseEventKind::Moved, Mode::ButtonMotion, 1, 1), None);
    assert_eq!(
        sgr(MouseEventKind::Moved, Mode::AnyMotion, 1, 1).unwrap(),
        b"\x1b[<35;2;2M"
    );
}

#[test]
fn the_default_encoding_uses_bytes_and_a_release_has_no_button() {
    let press = encode_mouse(
        MouseEventKind::Down(MouseButton::Left),
        NONE,
        0,
        0,
        Mode::PressRelease,
        Encoding::Default,
    );
    let release = encode_mouse(
        MouseEventKind::Up(MouseButton::Right),
        NONE,
        9,
        4,
        Mode::PressRelease,
        Encoding::Default,
    );

    assert_eq!(press.unwrap(), [0x1b, b'[', b'M', 32, 33, 33]);
    assert_eq!(release.unwrap(), [0x1b, b'[', b'M', 35, 42, 37]);
}

#[test]
fn the_default_encoding_cannot_reach_far_right_but_utf8_and_sgr_can() {
    let far = |encoding| {
        encode_mouse(
            MouseEventKind::Down(MouseButton::Left),
            NONE,
            300,
            0,
            Mode::PressRelease,
            encoding,
        )
    };

    assert_eq!(far(Encoding::Default), None, "x does not fit in a byte");
    let utf8 = far(Encoding::Utf8).unwrap();
    assert_eq!(&utf8[..3], b"\x1b[M");
    assert_eq!(std::str::from_utf8(&utf8[3..]).unwrap().chars().count(), 3);
    assert_eq!(far(Encoding::Sgr).unwrap(), b"\x1b[<0;301;1M");
}
