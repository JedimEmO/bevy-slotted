//! A scripted [`ScriptRuntime`] for the packs tests.
//!
//! Package A owns the mlua adapter; these tests must not wait for it, so the
//! runtime here simply replays commands the test wrote down, keyed by mod and
//! event name.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use slotted_script::{
    Limits, ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptId, ScriptRuntime, Stage,
};

/// What a fake script answers, per `(mod id, event name)`.
pub type Reply = Result<Vec<ScriptCommand>, ScriptError>;

/// The replies, shared with the test so it can rewrite them between loads.
#[derive(Debug, Default)]
pub struct Replies {
    pub by_event: HashMap<(String, String), Reply>,
    /// Every `(mod, event)` the host actually asked for, in order.
    pub calls: Vec<(String, String)>,
}

impl Replies {
    pub fn set(&mut self, mod_id: &str, event: &str, reply: Reply) {
        self.by_event
            .insert((mod_id.to_owned(), event.to_owned()), reply);
    }
}

/// A [`ScriptRuntime`] that returns scripted commands.
#[derive(Debug, Clone, Default)]
pub struct FakeRuntime {
    pub replies: Arc<Mutex<Replies>>,
    loaded: Arc<Mutex<Vec<Option<(ModId, Stage)>>>>,
}

impl FakeRuntime {
    pub fn new(replies: Arc<Mutex<Replies>>) -> Self {
        Self {
            replies,
            loaded: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Which scripts are still loaded, oldest first.
    pub fn live(&self) -> Vec<(ModId, Stage)> {
        self.loaded
            .lock()
            .expect("not poisoned")
            .iter()
            .flatten()
            .cloned()
            .collect()
    }
}

impl ScriptRuntime for FakeRuntime {
    fn load(
        &mut self,
        mod_id: &ModId,
        _name: &str,
        _source: &str,
        stage: Stage,
    ) -> Result<ScriptId, ScriptError> {
        let mut loaded = self.loaded.lock().expect("not poisoned");
        loaded.push(Some((mod_id.clone(), stage)));
        Ok(ScriptId(
            u32::try_from(loaded.len() - 1).expect("few scripts"),
        ))
    }

    fn unload(&mut self, id: ScriptId) {
        let mut loaded = self.loaded.lock().expect("not poisoned");
        if let Some(slot) = loaded.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    fn call(
        &mut self,
        id: ScriptId,
        event: &ScriptEvent,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let owner = {
            let loaded = self.loaded.lock().expect("not poisoned");
            match loaded.get(id.0 as usize).and_then(Option::as_ref) {
                Some((mod_id, _)) => mod_id.clone(),
                None => return Err(ScriptError::UnknownScript(id)),
            }
        };
        let mut replies = self.replies.lock().expect("not poisoned");
        replies
            .calls
            .push((owner.to_string(), event.name().to_owned()));
        match replies
            .by_event
            .get(&(owner.to_string(), event.name().to_owned()))
        {
            Some(Ok(commands)) => Ok(commands.clone()),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Vec::new()),
        }
    }

    fn set_limits(&mut self, _limits: Limits) {}
}
