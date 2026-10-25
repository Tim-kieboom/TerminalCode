use trcode_plugin_api::MAX_MESSAGE;

use crate::plugins::instance::{Compiled, Delivered, Failure, MAX_QUEUED_MESSAGES};

use super::*;

/// A plugin that sends `count` messages of `len` bytes (not valid messages, which is
/// no matter here) whenever it is called.
fn sender(count: u32, len: u32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
            (import "host" "send" (func $send (param i32 i32)))
            (memory (export "memory") 80)
            (func (export "alloc") (param i32) (result i32) (i32.const 65536))
            (func (export "handle") (param i32 i32) (local $n i32)
                (loop $again
                    (call $send (i32.const 0) (i32.const {len}))
                    (local.set $n (i32.add (local.get $n) (i32.const 1)))
                    (br_if $again (i32.lt_u (local.get $n) (i32.const {count}))))))"#
    ))
    .expect("the test module is valid")
}

fn deliver_to(wasm: &[u8]) -> Result<Delivered, Failure> {
    let mut instance = Compiled::new(wasm).unwrap().instantiate().unwrap();
    instance.deliver(ToPlugin::Refresh, 1_000_000_000)
}

#[test]
fn a_few_messages_are_all_delivered() {
    let delivered = deliver_to(&sender(10, 100)).unwrap();
    assert_eq!(delivered.rejected.len(), 10);
}

#[test]
fn too_many_messages_in_one_call_trap_the_plugin() {
    let error = deliver_to(&sender(MAX_QUEUED_MESSAGES as u32 + 1, 1)).unwrap_err();
    assert!(
        matches!(&error, Failure::Trap(reason) if reason.contains("messages")),
        "{error:?}"
    );
}

#[test]
fn too_many_bytes_in_one_call_trap_the_plugin() {
    let each = MAX_MESSAGE as u32;
    let error = deliver_to(&sender(5, each)).unwrap_err();
    assert!(matches!(&error, Failure::Trap(_)), "{error:?}");
}
