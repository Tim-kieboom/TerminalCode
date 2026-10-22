//! The calls between plugins that are open, and which plugins may be called. Pure
//! bookkeeping: each method says what to send to whom, and the runtime sends it.
//!
//! A call is open from the caller's [`FromPlugin::Call`] until the callee's last
//! [`FromPlugin::Reply`], the caller's [`FromPlugin::CallCancel`], or either plugin
//! being lost (it trapped or was turned off). What either says about a call that is
//! not open is dropped, which is what makes a cancel final.
//!
//! [`FromPlugin::Call`]: crate::plugin_api::FromPlugin::Call
//! [`FromPlugin::Reply`]: crate::plugin_api::FromPlugin::Reply
//! [`FromPlugin::CallCancel`]: crate::plugin_api::FromPlugin::CallCancel

use std::collections::{HashMap, HashSet};

use crate::plugin_api::{MAX_OPEN_CALLS, Refusal, ToPlugin};

/// A message for the plugin `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Outgoing {
    pub to: Box<str>,
    pub message: ToPlugin,
}

impl Outgoing {
    fn refused(to: &str, request: u64, reason: Refusal) -> Self {
        Self {
            to: to.into(),
            message: ToPlugin::Refused { request, reason },
        }
    }

    fn cancelled(to: &str, from: &str, request: u64) -> Self {
        Self {
            to: to.into(),
            message: ToPlugin::CallCancelled {
                from: from.into(),
                request,
            },
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Calls {
    /// The plugins that may be called.
    serving: HashSet<Box<str>>,
    /// The open calls: caller, then the caller's request number, to the callee.
    open: HashMap<Box<str>, HashMap<u64, Box<str>>>,
}

impl Calls {
    /// `plugin` may be called from now on.
    pub(super) fn serve(&mut self, plugin: &str) {
        self.serving.insert(plugin.into());
    }

    /// Opens the call `request` of `caller` to `to`; the message for the callee, or
    /// why the call cannot be made.
    pub(super) fn open(
        &mut self,
        caller: &str,
        request: u64,
        to: &str,
        method: String,
        payload: String,
    ) -> Result<Outgoing, Refusal> {
        if to == caller {
            return Err(Refusal::BadArgument {
                detail: "a plugin cannot call itself".to_owned(),
            });
        }
        if !self.serving.contains(to) {
            return Err(Refusal::NoSuchPlugin);
        }
        let Some(calls) = self.open.get_mut(caller) else {
            self.open
                .insert(caller.into(), HashMap::from([(request, to.into())]));
            return Ok(call_message(caller, request, to, method, payload));
        };
        if calls.contains_key(&request) {
            return Err(Refusal::BadArgument {
                detail: format!("the call {request} is open already"),
            });
        }
        if calls.len() >= MAX_OPEN_CALLS {
            return Err(Refusal::Busy);
        }
        calls.insert(request, to.into());
        Ok(call_message(caller, request, to, method, payload))
    }

    /// A part of the answer from `from` to the call `request` of `caller`: the
    /// message for the caller, or nothing when no such call is open with `from`. The
    /// call ends with the part that is `last`.
    pub(super) fn reply(
        &mut self,
        from: &str,
        caller: &str,
        request: u64,
        payload: String,
        last: bool,
    ) -> Option<Outgoing> {
        let calls = self.open.get_mut(caller)?;
        if calls.get(&request)?.as_ref() != from {
            return None;
        }
        if last {
            calls.remove(&request);
            self.forget_if_empty(caller);
        }
        Some(Outgoing {
            to: caller.into(),
            message: ToPlugin::Reply {
                from: from.into(),
                request,
                payload,
                last,
            },
        })
    }

    /// `caller` gives up its call `request`: the message for the callee, when the
    /// call was open.
    pub(super) fn cancel(&mut self, caller: &str, request: u64) -> Option<Outgoing> {
        let callee = self.open.get_mut(caller)?.remove(&request)?;
        self.forget_if_empty(caller);
        Some(Outgoing::cancelled(&callee, caller, request))
    }

    /// The call `request` of `caller` cannot be delivered (its callee is not there):
    /// the message that tells the caller.
    pub(super) fn abandon(&mut self, caller: &str, request: u64) -> Option<Outgoing> {
        self.open.get_mut(caller)?.remove(&request)?;
        self.forget_if_empty(caller);
        Some(Outgoing::refused(caller, request, Refusal::Gone))
    }

    /// `plugin` trapped and starts again with nothing in its memory, so the calls it
    /// was answering are lost (their callers are told) and the calls it made are
    /// forgotten (their callees are told). It can still be called.
    pub(super) fn reset(&mut self, plugin: &str) -> Vec<Outgoing> {
        let mut told = Vec::new();
        let mut lost: Vec<(Box<str>, u64)> = Vec::new();
        for (caller, calls) in &mut self.open {
            calls.retain(|request, callee| {
                let keep = callee.as_ref() != plugin;
                if !keep {
                    lost.push((caller.clone(), *request));
                }
                keep
            });
        }
        lost.sort();
        told.extend(
            lost.iter()
                .map(|(caller, request)| Outgoing::refused(caller, *request, Refusal::Gone)),
        );
        if let Some(made) = self.open.remove(plugin) {
            let mut made: Vec<(u64, Box<str>)> = made.into_iter().collect();
            made.sort();
            told.extend(
                made.iter()
                    .map(|(request, callee)| Outgoing::cancelled(callee, plugin, *request)),
            );
        }
        self.open.retain(|_, calls| !calls.is_empty());
        told
    }

    /// `plugin` is gone (turned off, replaced or broken): everything of
    /// [`Calls::reset`], and it cannot be called any more.
    pub(super) fn remove(&mut self, plugin: &str) -> Vec<Outgoing> {
        self.serving.remove(plugin);
        self.reset(plugin)
    }

    fn forget_if_empty(&mut self, caller: &str) {
        if self.open.get(caller).is_some_and(HashMap::is_empty) {
            self.open.remove(caller);
        }
    }
}

fn call_message(caller: &str, request: u64, to: &str, method: String, payload: String) -> Outgoing {
    Outgoing {
        to: to.into(),
        message: ToPlugin::Call {
            from: caller.into(),
            request,
            method,
            payload,
        },
    }
}

#[cfg(test)]
mod tests;
