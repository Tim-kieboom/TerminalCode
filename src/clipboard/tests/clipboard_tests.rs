use crate::clipboard::system::Memory;
use crate::clipboard::{Clipboard, Notice, Register, RegisterKind, System};

fn clipboard(memory: Memory) -> Clipboard {
    Clipboard::new(System::Memory(memory))
}

fn text(fetched: Option<crate::clipboard::Fetched>) -> Option<String> {
    fetched.map(|fetched| fetched.register.text().to_owned())
}

#[test]
fn an_empty_clipboard_has_nothing_to_paste() {
    let mut clipboard = clipboard(Memory::default());

    assert_eq!(clipboard.get(), None);
}

#[test]
fn copy_then_paste_returns_the_register_with_its_kind() {
    let mut clipboard = clipboard(Memory::default());
    clipboard.set(Register::linewise("line\n"));

    let fetched = clipboard.get().unwrap();

    assert_eq!(fetched.register, Register::linewise("line\n"));
    assert_eq!(fetched.notice, None);
}

#[test]
fn copy_also_writes_the_system_clipboard() {
    let mut clipboard = clipboard(Memory::default());

    clipboard.set(Register::charwise("abc"));

    let System::Memory(memory) = &clipboard.system else {
        unreachable!()
    };
    assert_eq!(memory.content.as_deref(), Some("abc"));
}

#[test]
fn something_copied_elsewhere_wins_over_the_register() {
    let mut clipboard = clipboard(Memory::default());
    clipboard.set(Register::linewise("from the editor\n"));

    // Another program replaces the system clipboard.
    let System::Memory(memory) = &mut clipboard.system else {
        unreachable!()
    };
    memory.content = Some("from the browser".to_owned());

    let fetched = clipboard.get().unwrap();
    assert_eq!(fetched.register, Register::charwise("from the browser"));
}

#[test]
fn a_foreign_copy_wins_even_before_any_editor_copy() {
    let mut clipboard = clipboard(Memory {
        content: Some("from the browser".to_owned()),
        ..Memory::default()
    });

    assert_eq!(text(clipboard.get()).as_deref(), Some("from the browser"));
}

#[test]
fn the_register_comes_back_when_the_system_clipboard_still_holds_our_text() {
    let mut clipboard = clipboard(Memory::default());
    clipboard.set(Register::linewise("a\n"));
    clipboard.get();

    let fetched = clipboard.get().unwrap();

    assert_eq!(fetched.register.kind(), RegisterKind::Linewise);
}

#[test]
fn a_newer_editor_copy_wins_over_an_older_foreign_one() {
    let mut clipboard = clipboard(Memory::default());
    clipboard.set(Register::charwise("first"));
    let System::Memory(memory) = &mut clipboard.system else {
        unreachable!()
    };
    memory.content = Some("foreign".to_owned());

    clipboard.set(Register::charwise("second"));

    assert_eq!(text(clipboard.get()).as_deref(), Some("second"));
}

#[test]
fn an_unreadable_system_clipboard_falls_back_to_the_register_with_one_notice() {
    let mut clipboard = clipboard(Memory {
        readable: false,
        ..Memory::default()
    });
    clipboard.set(Register::charwise("kept"));

    let first = clipboard.get().unwrap();
    let second = clipboard.get().unwrap();

    assert_eq!(first.register.text(), "kept");
    assert_eq!(first.notice, Some(Notice::SystemUnreadable));
    assert_eq!(second.register.text(), "kept");
    assert_eq!(second.notice, None);
}

#[test]
fn an_unreadable_clipboard_with_nothing_copied_stays_silent() {
    let mut clipboard = clipboard(Memory {
        readable: false,
        ..Memory::default()
    });

    assert_eq!(clipboard.get(), None);
}

#[test]
fn a_failed_write_still_keeps_the_register() {
    let mut clipboard = clipboard(Memory {
        writable: false,
        readable: false,
        ..Memory::default()
    });

    clipboard.set(Register::charwise("local only"));

    assert_eq!(text(clipboard.get()).as_deref(), Some("local only"));
}

#[test]
fn the_internal_only_clipboard_works_without_any_system_clipboard() {
    let mut clipboard = Clipboard::internal_only();
    clipboard.set(Register::charwise("x"));

    assert_eq!(text(clipboard.get()).as_deref(), Some("x"));
}
