use terminal_code::error::IdeResult;

fn main() {
    if let Err(err) = start() {
        eprint!("error: {err}")
    }
}

fn start() -> IdeResult {
    set_restore_on_panic();

    let terminal = ratatui::init();
    let result = terminal_code::run(terminal);
    ratatui::restore();
    result
}

fn set_restore_on_panic() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        original_hook(info);
    }));
}
