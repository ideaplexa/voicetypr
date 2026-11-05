//! Resources belong to the generation published before asynchronous startup.
use std::sync::Mutex;

pub(crate) struct CaptureSlot<T>(Mutex<(Option<u64>, Option<T>)>);

impl<T> Default for CaptureSlot<T> {
    fn default() -> Self {
        Self(Mutex::new((None, None)))
    }
}

impl<T> CaptureSlot<T> {
    pub(crate) fn begin(&self, generation: u64) {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *slot = (Some(generation), None);
    }

    pub(crate) fn replace(&self, generation: u64, value: T) -> bool {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // Check while holding the SAME lock as replacement, not before it.
        if slot.0 != Some(generation) {
            return false;
        }
        slot.1 = Some(value);
        true
    }

    pub(crate) fn take(&self, generation: u64) -> Option<T> {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        (slot.0 == Some(generation))
            .then(|| slot.1.take())
            .flatten()
    }
}

#[cfg(test)]
impl<T: Clone> CaptureSlot<T> {
    pub(crate) fn get(&self, generation: u64) -> Option<T> {
        let slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        (slot.0 == Some(generation))
            .then(|| slot.1.clone())
            .flatten()
    }
}

#[cfg(test)]
mod tests;
