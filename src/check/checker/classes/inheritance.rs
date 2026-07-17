//! Published class inheritance identity helpers.

use super::super::context::Pass;
use crate::types::repr::ClassId;

impl<Ticket: Copy + PartialEq> Pass<'_, '_, Ticket> {
    pub(super) fn declaring_class_name(&self, class_id: ClassId) -> &str {
        self.class_names
            .get(&class_id)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub(super) fn is_class_or_subclass(&self, class_id: ClassId, ancestor: ClassId) -> bool {
        let mut current = class_id;
        let mut visited = rustc_hash::FxHashSet::default();
        loop {
            if current == ancestor {
                return true;
            }
            if !visited.insert(current) {
                return false;
            }
            match self.class_parents.get(&current).copied() {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }
}
