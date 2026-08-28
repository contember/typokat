//! Publisher-session durable terminal cache.

#[cfg(any(test, feature = "test-utils"))]
use super::observation;
use super::terminal::ProductTerminal;
use std::cell::RefCell;
use std::collections::BTreeMap;

pub(super) struct DurableCache {
    #[cfg(any(test, feature = "test-utils"))]
    publisher: u64,
    terminals: RefCell<BTreeMap<usize, ProductTerminal>>,
}

impl DurableCache {
    pub(super) fn new(publisher: u64) -> Self {
        #[cfg(not(any(test, feature = "test-utils")))]
        let _ = publisher;
        Self {
            #[cfg(any(test, feature = "test-utils"))]
            publisher,
            terminals: RefCell::new(BTreeMap::new()),
        }
    }

    pub(super) fn get(&self, product_group: usize) -> Option<ProductTerminal> {
        let terminal = self.terminals.borrow().get(&product_group).cloned();
        #[cfg(any(test, feature = "test-utils"))]
        if terminal.is_some() {
            observation::record_cache_hit(self.publisher, product_group);
        }
        terminal
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn peek(&self, product_group: usize) -> Option<ProductTerminal> {
        self.terminals.borrow().get(&product_group).cloned()
    }

    pub(super) fn contains(&self, product_group: usize) -> bool {
        self.terminals.borrow().contains_key(&product_group)
    }

    pub(super) fn commit(&self, product_group: usize, terminal: ProductTerminal) -> bool {
        let mut terminals = self.terminals.borrow_mut();
        if terminals.contains_key(&product_group) {
            return false;
        }
        terminals.insert(product_group, terminal);
        drop(terminals);
        #[cfg(any(test, feature = "test-utils"))]
        observation::record_cache_write(self.publisher, product_group);
        true
    }
}
