//! The glue that makes a [`Plugin`](crate::Plugin) a WebAssembly module the editor
//! can run: the exports `alloc` and `handle` and the import `host.send`. It exists
//! only when building for `wasm32`; use it through [`export_plugin!`](crate::export_plugin).
//!
//! ```ignore
//! #[derive(Default)]
//! struct Hello;
//!
//! impl trcode_plugin_api::Plugin for Hello {
//!     fn handle(&mut self, message: ToPlugin) -> Vec<FromPlugin> { /* ... */ }
//! }
//!
//! trcode_plugin_api::export_plugin!(Hello);
//! ```

/// Makes the plugin type (which must be `Default`) the module's `alloc` and
/// `handle`. A panic stops the module (a trap), and the editor starts it again.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            #[unsafe(no_mangle)]
            pub extern "C" fn alloc(len: usize) -> *mut u8 {
                $crate::guest::inbox(len)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn handle(ptr: *const u8, len: usize) {
                $crate::guest::deliver::<$plugin>(ptr, len)
            }
        };
    };
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::any::Any;
    use std::cell::RefCell;

    use crate::{FromPlugin, Plugin, ToPlugin};

    #[link(wasm_import_module = "host")]
    unsafe extern "C" {
        fn send(ptr: *const u8, len: usize);
    }

    thread_local! {
        /// Where the editor writes the message it sends.
        static INBOX: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        /// The plugin, made when the first message comes.
        static PLUGIN: RefCell<Option<Box<dyn Any>>> = const { RefCell::new(None) };
    }

    /// Room for a message of `len` bytes; where it goes.
    pub fn inbox(len: usize) -> *mut u8 {
        INBOX.with(|inbox| {
            let mut inbox = inbox.borrow_mut();
            inbox.resize(len, 0);
            inbox.as_mut_ptr()
        })
    }

    /// Hands the message in the inbox to the plugin and sends what it answers.
    pub fn deliver<P: Plugin + Default + 'static>(_ptr: *const u8, len: usize) {
        let message = INBOX.with(|inbox| {
            let inbox = inbox.borrow();
            ToPlugin::from_bytes(&inbox[..len.min(inbox.len())])
        });
        // A message that is not one is dropped; the editor only sends good ones.
        let Ok(message) = message else {
            return;
        };
        let replies = PLUGIN.with(|slot| {
            let mut slot = slot.borrow_mut();
            let plugin = slot.get_or_insert_with(|| Box::new(P::default()));
            match plugin.downcast_mut::<P>() {
                Some(plugin) => plugin.handle(message),
                None => Vec::new(),
            }
        });
        for reply in replies {
            send_message(&reply);
        }
    }

    fn send_message(message: &FromPlugin) {
        // A message that is too big to send is dropped.
        if let Ok(bytes) = message.to_bytes() {
            // SAFETY: the editor reads `len` bytes at `ptr` before the call returns.
            unsafe { send(bytes.as_ptr(), bytes.len()) };
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{deliver, inbox};
