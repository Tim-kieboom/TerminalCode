//! Plugins for the tests, written as WebAssembly text: they answer every message
//! with a view that has the message in it, so a test reads exactly what reached
//! the plugin, and they can be made to ask for something or to trap.

const LINES_PREFIX: &str = r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":""#;
const LINES_SUFFIX: &str = r#"","slot":""}]}}"#;
const INPUT_PREFIX: &str = r#"{"type":"set_view","view":{"type":"input","prompt":"> ","value":""#;
const INPUT_SUFFIX: &str = r#"","cursor":0}}"#;

/// What a plugin shows before it has heard anything but `start`.
#[derive(Clone, Copy)]
pub(crate) enum First {
    /// What it was sent, as lines.
    Echo,
    /// What it was sent, as a line to type into.
    EchoInInput,
    /// A line to type into, and what it was sent as lines from then on.
    InputThenLines,
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

pub(crate) struct Echo {
    first: First,
    request: Option<String>,
    asks: Asks,
    traps_on_refresh: bool,
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
        }
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
            First::EchoInInput => (INPUT_PREFIX, INPUT_SUFFIX),
            First::Echo | First::InputThenLines => (LINES_PREFIX, LINES_SUFFIX),
        };
        let start_view = format!("{INPUT_PREFIX}{INPUT_SUFFIX}");
        let on_start = match self.first {
            First::InputThenLines => format!(
                "(if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 9))) (i32.const 115))
                   (then (call $send (i32.const 3072) (i32.const {})) (return)))",
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
                   (then (call $send (i32.const 4096) (i32.const {}))))",
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
        let (prefix_data, suffix_data, start_data, request_data) = (
            data(1024, prefix),
            data(1536, suffix),
            data(3072, &start_view),
            data(4096, &request),
        );
        wat::parse_str(format!(
            r#"(module
                (import "host" "send" (func $send (param i32 i32)))
                (memory (export "memory") 4)
                {prefix_data}
                {suffix_data}
                {start_data}
                {request_data}
                (func (export "alloc") (param i32) (result i32) (i32.const 65536))
                (func (export "handle") (param $ptr i32) (param $len i32)
                  (local $out i32) (local $i i32) (local $b i32)
                  {on_start}
                  {trap}
                  {ask}
                  (local.set $out (i32.const 8192))
                  (memory.copy (local.get $out) (i32.const 1024) (i32.const {prefix_len}))
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
                  (memory.copy (local.get $out) (i32.const 1536) (i32.const {suffix_len}))
                  (local.set $out (i32.add (local.get $out) (i32.const {suffix_len})))
                  (call $send (i32.const 8192) (i32.sub (local.get $out) (i32.const 8192)))))"#,
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
