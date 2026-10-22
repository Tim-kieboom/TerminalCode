//! Plugins for the tests, written as WebAssembly text: they answer every message
//! with a view that has the message in it, so a test reads exactly what reached
//! the plugin, and they can be made to ask for something or to trap.

const LINES_PREFIX: &str = r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":""#;
const LINES_SUFFIX: &str = r#"","slot":""}]}}"#;
const INPUT_PREFIX: &str = r#"{"type":"set_view","view":{"type":"input","prompt":"> ","value":""#;
const INPUT_SUFFIX: &str = r#"","cursor":0}}"#;
const COLUMN_PREFIX: &str = r#"{"type":"set_view","view":{"type":"column","parts":[{"type":"buttons","buttons":[{"id":"commit","label":"Commit"},{"id":"push","label":"Push","enabled":false}]},{"type":"rows","lines":[{"text":"row 0","slot":""},{"text":"row 1","slot":""}],"selected":0},{"type":"input","prompt":"> ","value":""#;
const COLUMN_SUFFIX: &str = r#"","cursor":0,"active":false}]}}"#;
const ROWS_SUFFIX: &str = r#"","slot":""}],"selected":40}}"#;

fn rows_prefix() -> String {
    let rows: Vec<String> = (0..40)
        .map(|n| format!(r#"{{"text":"row {n}","slot":""}}"#))
        .collect();
    format!(
        r#"{{"type":"set_view","view":{{"type":"rows","lines":[{},{{"text":""#,
        rows.join(",")
    )
}

/// Where things are in the memory of a fixture plugin.
const PREFIX_AT: u32 = 1024;
const SUFFIX_AT: u32 = 4096;
const START_AT: u32 = 4608;
const REQUEST_AT: u32 = 5120;
const OUT_AT: u32 = 12288;

/// What a plugin shows before it has heard anything but `start`.
#[derive(Clone, Copy)]
pub(crate) enum First {
    /// What it was sent, as lines.
    Echo,
    /// What it was sent, as a line to type into.
    EchoInInput,
    /// A line to type into, and what it was sent as lines from then on.
    InputThenLines,
    /// Forty rows, `row 0` to `row 39`, and what it was sent as the row after them,
    /// which is the selected one.
    EchoInRows,
    /// A column: two buttons (`commit`, and `push` that cannot be pressed), two rows
    /// and a line to type into that is not active, with what it was sent as its text.
    EchoInColumn,
}

/// When a plugin asks for what it was told to ask for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asks {
    Never,
    /// When it is sent an `action` message.
    OnAction,
    /// Whenever it is sent anything: an answer makes it ask again, for ever.
    Always,
}

/// What a plugin does when the byte of the message it was sent at some place is some value.
enum Then {
    Send(String),
    Trap,
}

struct Trigger {
    /// Each place in the message and the byte that must be there, all of them.
    when: Vec<(u32, u8)>,
    then: Then,
}

/// The place in a message where the type starts: `{"type":"` is nine bytes.
pub(crate) const TYPE_AT: u32 = 9;
/// The place of the first letter of the action in `{"type":"action","action":"x"}`.
pub(crate) const ACTION_AT: u32 = 27;
/// The most things one plugin does on a byte; each has a kilobyte of memory.
const MOST_TRIGGERS: usize = 5;

pub(crate) struct Echo {
    first: First,
    request: Option<String>,
    asks: Asks,
    traps_on_refresh: bool,
    triggers: Vec<Trigger>,
}

pub(crate) fn echo(first: First) -> Vec<u8> {
    Echo::new(first).build()
}

impl Echo {
    pub(crate) fn new(first: First) -> Self {
        Self {
            first,
            request: None,
            asks: Asks::Never,
            traps_on_refresh: false,
            triggers: Vec::new(),
        }
    }

    /// Sends `request` when it is sent an `action` whose id starts with `letter`.
    pub(crate) fn asks_on_action_named(mut self, letter: u8, request: &str) -> Self {
        // Only an `action` has the name of one at that place (the bytes after a short
        // message are what an earlier one left).
        let when = vec![(TYPE_AT, b'a'), (ACTION_AT, letter)];
        self.push(when, Then::Send(request.to_owned()));
        self
    }

    /// Sends `request` when the byte of the message it is sent at `at` is `byte`
    /// (the type of the message starts at 9).
    pub(crate) fn asks_on_byte(mut self, at: u32, byte: u8, request: &str) -> Self {
        self.push(vec![(at, byte)], Then::Send(request.to_owned()));
        self
    }

    /// Loops for ever when the byte of the message it is sent at `at` is `byte`.
    pub(crate) fn traps_on_byte(mut self, at: u32, byte: u8) -> Self {
        self.push(vec![(at, byte)], Then::Trap);
        self
    }

    fn push(&mut self, when: Vec<(u32, u8)>, then: Then) {
        assert!(self.triggers.len() < MOST_TRIGGERS, "too many triggers");
        self.triggers.push(Trigger { when, then });
    }

    /// Sends `request` (a message of the plugin API) whenever it is sent an
    /// `action`, before it shows what it was sent.
    pub(crate) fn asks_on_action(mut self, request: &str) -> Self {
        self.request = Some(request.to_owned());
        self.asks = Asks::OnAction;
        self
    }

    /// Sends `request` whenever it is sent anything at all.
    pub(crate) fn asks_always(mut self, request: &str) -> Self {
        self.request = Some(request.to_owned());
        self.asks = Asks::Always;
        self
    }

    /// Loops for ever when it is sent a `refresh`.
    pub(crate) fn traps_on_refresh(mut self) -> Self {
        self.traps_on_refresh = true;
        self
    }

    pub(crate) fn build(self) -> Vec<u8> {
        let (prefix, suffix) = match self.first {
            First::EchoInInput => (INPUT_PREFIX.to_owned(), INPUT_SUFFIX),
            First::EchoInRows => (rows_prefix(), ROWS_SUFFIX),
            First::EchoInColumn => (COLUMN_PREFIX.to_owned(), COLUMN_SUFFIX),
            First::Echo | First::InputThenLines => (LINES_PREFIX.to_owned(), LINES_SUFFIX),
        };
        let start_view = format!("{INPUT_PREFIX}{INPUT_SUFFIX}");
        let on_start = match self.first {
            First::InputThenLines => format!(
                "(if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 9))) (i32.const 115))
                   (then (call $send (i32.const {START_AT}) (i32.const {})) (return)))",
                start_view.len()
            ),
            _ => String::new(),
        };
        let request = self.request.clone().unwrap_or_default();
        let ask = match self.asks {
            Asks::Never => String::new(),
            Asks::Always => format!(
                "(call $send (i32.const 4096) (i32.const {}))",
                request.len()
            ),
            Asks::OnAction => format!(
                "(if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 9))) (i32.const 97))
                   (then (call $send (i32.const {REQUEST_AT}) (i32.const {}))))",
                request.len()
            ),
        };
        // `refresh` and `refused` both start with `ref`; the fourth letter differs.
        let trap = match self.traps_on_refresh {
            false => String::new(),
            true => "(if (i32.and
                       (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 9))) (i32.const 114))
                       (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 12))) (i32.const 114)))
                   (then (loop $forever (br $forever))))"
                .to_owned(),
        };
        let (mut trigger_data, mut trigger_code) = (String::new(), String::new());
        for (n, trigger) in self.triggers.iter().enumerate() {
            let at = REQUEST_AT + 1024 * (n as u32 + 1);
            let tests: Vec<String> = trigger
                .when
                .iter()
                .map(|(at, byte)| {
                    format!(
                        "(i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const {at}))) (i32.const {byte}))"
                    )
                })
                .collect();
            let is = tests
                .into_iter()
                .reduce(|all, test| format!("(i32.and {all} {test})"))
                .unwrap_or_default();
            match &trigger.then {
                Then::Send(request) => {
                    trigger_data += &data(at, request);
                    trigger_code += &format!(
                        "(if {is} (then (call $send (i32.const {at}) (i32.const {}))))",
                        request.len()
                    );
                }
                Then::Trap => {
                    trigger_code += &format!("(if {is} (then (loop $forever (br $forever))))");
                }
            }
        }
        let (prefix_data, suffix_data, start_data, request_data) = (
            data(PREFIX_AT, &prefix),
            data(SUFFIX_AT, suffix),
            data(START_AT, &start_view),
            data(REQUEST_AT, &request),
        );
        wat::parse_str(format!(
            r#"(module
                (import "host" "send" (func $send (param i32 i32)))
                (memory (export "memory") 4)
                {prefix_data}
                {suffix_data}
                {start_data}
                {request_data}
                {trigger_data}
                (func (export "alloc") (param i32) (result i32) (i32.const 65536))
                (func (export "handle") (param $ptr i32) (param $len i32)
                  (local $out i32) (local $i i32) (local $b i32)
                  {on_start}
                  {trap}
                  {ask}
                  {trigger_code}
                  (local.set $out (i32.const {OUT_AT}))
                  (memory.copy (local.get $out) (i32.const {PREFIX_AT}) (i32.const {prefix_len}))
                  (local.set $out (i32.add (local.get $out) (i32.const {prefix_len})))
                  (block $done
                    (loop $next
                      (br_if $done (i32.ge_u (local.get $i) (local.get $len)))
                      (local.set $b (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
                      (if (i32.or (i32.eq (local.get $b) (i32.const 34))
                                  (i32.eq (local.get $b) (i32.const 92)))
                        (then
                          (i32.store8 (local.get $out) (i32.const 92))
                          (local.set $out (i32.add (local.get $out) (i32.const 1)))))
                      (i32.store8 (local.get $out) (local.get $b))
                      (local.set $out (i32.add (local.get $out) (i32.const 1)))
                      (local.set $i (i32.add (local.get $i) (i32.const 1)))
                      (br $next)))
                  (memory.copy (local.get $out) (i32.const {SUFFIX_AT}) (i32.const {suffix_len}))
                  (local.set $out (i32.add (local.get $out) (i32.const {suffix_len})))
                  (call $send (i32.const {OUT_AT}) (i32.sub (local.get $out) (i32.const {OUT_AT})))))"#,
            prefix_len = prefix.len(),
            suffix_len = suffix.len(),
        ))
        .expect("the test module is valid")
    }
}

fn data(at: u32, text: &str) -> String {
    format!(
        "(data (i32.const {at}) \"{}\")",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    )
}
