use std::collections::VecDeque;

use super::*;

/// Hands out its chunks one by one, then nothing.
struct Fake(VecDeque<Vec<u8>>);

impl Fake {
    fn new(chunks: &[&[u8]]) -> Self {
        Self(chunks.iter().map(|chunk| chunk.to_vec()).collect())
    }
}

impl Input for Fake {
    fn read(&mut self, _: Duration) -> Option<Vec<u8>> {
        self.0.pop_front()
    }
}

const LONG: Duration = Duration::from_secs(5);

#[test]
fn mouse_reports_are_read_until_the_answer_and_not_past_it() {
    let mut input = Fake::new(&[
        b"\x1b[<35;95;23M\x1b[<35;94;22M",
        b"\x1b[<35;93;22M\x1b[0n",
        b"typed after",
    ]);

    assert_eq!(until_answered(&mut input, LONG), Drained::Answered);
    assert_eq!(input.0.len(), 1, "what came after the answer is left alone");
}

#[test]
fn an_answer_cut_between_two_reads_is_found() {
    let mut input = Fake::new(&[b"\x1b[<35;1;1M\x1b[", b"0", b"n"]);

    assert_eq!(until_answered(&mut input, LONG), Drained::Answered);
}

#[test]
fn without_an_answer_it_stops_when_the_input_is_quiet() {
    let mut input = Fake::new(&[b"\x1b[<35;95;23M"]);

    assert_eq!(until_answered(&mut input, LONG), Drained::TimedOut);
}

#[test]
fn a_flood_of_reports_without_an_answer_stops_at_the_timeout() {
    struct Endless;
    impl Input for Endless {
        fn read(&mut self, _: Duration) -> Option<Vec<u8>> {
            std::thread::sleep(Duration::from_millis(5));
            Some(b"\x1b[<35;95;23M".to_vec())
        }
    }

    let start = Instant::now();
    let result = until_answered(&mut Endless, Duration::from_millis(30));

    assert_eq!(result, Drained::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn text_that_only_looks_like_the_start_of_the_answer_is_not_the_answer() {
    let mut input = Fake::new(&[b"\x1b[0", b"m"]);

    assert_eq!(until_answered(&mut input, LONG), Drained::TimedOut);
}
