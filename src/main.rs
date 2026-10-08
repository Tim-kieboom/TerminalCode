use terminal_code::error::IdeResult;
use terminal_code::terminal;

fn main() {
    if let Err(err) = start() {
        eprintln!("error: {err}")
    }
}

fn start() -> IdeResult {
    set_restore_on_panic();

    let paths = terminal_code::paths_from_args(std::env::args_os());
    let (terminal, capabilities) = terminal::init();
    let result = terminal_code::run(terminal, &paths, capabilities);
    terminal::restore();
    result
}

fn set_restore_on_panic() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        terminal::restore();
        original_hook(info);
    }));
}
