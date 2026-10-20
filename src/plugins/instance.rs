//! One running plugin: a `wasmi` instance with a fuel budget and a memory limit,
//! and the calls the editor makes into it.

use wasmi::{
    Caller, Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

use super::{LOAD_FUEL, MEMORY_LIMIT};
use crate::plugin_api::{FromPlugin, MAX_MESSAGE, ToPlugin};

/// Why a call into a plugin did not work out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Failure {
    /// The plugin ran and failed: out of fuel, a trap, a message that is too big,
    /// a pointer outside its memory. Counts against the plugin; it is started again.
    Trap(String),
    /// The plugin cannot run at all: not a valid module, a missing export or
    /// import. Starting it again would not help.
    Broken(String),
}

impl Failure {
    /// A short reason for the user.
    pub(super) fn reason(&self) -> &str {
        match self {
            Self::Trap(reason) | Self::Broken(reason) => reason,
        }
    }
}

/// What the plugin said during one call.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Delivered {
    pub messages: Vec<FromPlugin>,
    /// What was said that could not be read, one reason each.
    pub rejected: Vec<String>,
}

/// What host functions reach: the memory limit and what the plugin sent.
struct Host {
    limits: StoreLimits,
    sent: Vec<Vec<u8>>,
}

/// A compiled module, which can be started again as often as needed.
pub(super) struct Compiled {
    engine: Engine,
    module: Module,
}

impl Compiled {
    pub(super) fn new(wasm: &[u8]) -> Result<Self, Failure> {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm)
            .map_err(|error| Failure::Broken(format!("not a valid module: {error}")))?;
        Ok(Self { engine, module })
    }

    /// A fresh instance, its start function run within the loading budget.
    pub(super) fn instantiate(&self) -> Result<Instance, Failure> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(MEMORY_LIMIT)
            .instances(1)
            .build();
        let mut store = Store::new(
            &self.engine,
            Host {
                limits,
                sent: Vec::new(),
            },
        );
        store.limiter(|host| &mut host.limits);
        store
            .set_fuel(LOAD_FUEL)
            .map_err(|error| Failure::Broken(error.to_string()))?;

        let mut linker = <Linker<Host>>::new(&self.engine);
        linker
            .func_wrap("host", "send", send)
            .map_err(|error| Failure::Broken(error.to_string()))?;
        // Linking and the plugin's start function run in one step; a trap is the
        // plugin failing, anything else is a module that cannot be used.
        let instance = linker
            .instantiate_and_start(&mut store, &self.module)
            .map_err(|error| match error.as_trap_code() {
                Some(_) => Failure::Trap(describe(&error)),
                None => Failure::Broken(format!("cannot be started: {error}")),
            })?;

        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| Failure::Broken("it exports no `memory`".into()))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "alloc")
            .map_err(|error| Failure::Broken(format!("its `alloc` is not usable: {error}")))?;
        let handle = instance
            .get_typed_func::<(i32, i32), ()>(&store, "handle")
            .map_err(|error| Failure::Broken(format!("its `handle` is not usable: {error}")))?;
        Ok(Instance {
            store,
            memory,
            alloc,
            handle,
        })
    }
}

/// `host.send(ptr, len)`: the plugin hands the editor a message.
fn send(mut caller: Caller<'_, Host>, ptr: i32, len: i32) -> Result<(), wasmi::Error> {
    let len = usize::try_from(len).map_err(|_| wasmi::Error::new("a negative message length"))?;
    if len > MAX_MESSAGE {
        return Err(wasmi::Error::new(format!(
            "a message of {len} bytes, more than the {MAX_MESSAGE} allowed"
        )));
    }
    let ptr = usize::try_from(ptr).map_err(|_| wasmi::Error::new("a negative pointer"))?;
    let memory = caller
        .get_export("memory")
        .and_then(wasmi::Extern::into_memory)
        .ok_or_else(|| wasmi::Error::new("it has no memory"))?;
    let mut bytes = vec![0; len];
    memory
        .read(&caller, ptr, &mut bytes)
        .map_err(|_| wasmi::Error::new("a message outside its memory"))?;
    caller.data_mut().sent.push(bytes);
    Ok(())
}

pub(super) struct Instance {
    store: Store<Host>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    handle: TypedFunc<(i32, i32), ()>,
}

impl Instance {
    /// Sends the plugin a message and collects what it said while handling it,
    /// within `fuel`.
    pub(super) fn deliver(&mut self, message: ToPlugin, fuel: u64) -> Result<Delivered, Failure> {
        let bytes = message
            .to_bytes()
            .map_err(|error| Failure::Broken(error.to_string()))?;
        let len = i32::try_from(bytes.len())
            .map_err(|_| Failure::Broken("a message that is too long".into()))?;
        self.store
            .set_fuel(fuel)
            .map_err(|error| Failure::Broken(error.to_string()))?;

        let ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|error| Failure::Trap(describe(&error)))?;
        let at = usize::try_from(ptr)
            .map_err(|_| Failure::Trap("`alloc` gave a negative pointer".into()))?;
        self.memory
            .write(&mut self.store, at, &bytes)
            .map_err(|_| Failure::Trap("`alloc` gave a place outside its memory".into()))?;
        self.handle
            .call(&mut self.store, (ptr, len))
            .map_err(|error| Failure::Trap(describe(&error)))?;

        let mut delivered = Delivered::default();
        for bytes in std::mem::take(&mut self.store.data_mut().sent) {
            match FromPlugin::from_bytes(&bytes) {
                Ok(message) => delivered.messages.push(message),
                Err(error) => delivered.rejected.push(error.to_string()),
            }
        }
        Ok(delivered)
    }
}

/// What went wrong in a call, for a message to the user.
fn describe(error: &wasmi::Error) -> String {
    match error.as_trap_code() {
        Some(wasmi::TrapCode::OutOfFuel) => "it ran out of budget".to_owned(),
        _ => error.to_string(),
    }
}
