use dashmap::DashMap;
use objc2_core_foundation::CGRect;
use serde::{Deserialize, Serialize};

use crate::model::tx_store::WindowTxStore;
use crate::sys::geometry::SameAs;
use crate::sys::window_server::WindowServerId;

/// A per-window counter that tracks the last time the reactor sent a request to
/// change the window frame.
#[derive(Default, Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionId(u32);

impl TransactionId {
    pub fn next(self) -> Self { Self(self.0.wrapping_add(1)) }
}

/// How many times a target the app answered with another frame may be re-sent.
const MAX_REJECTED_TARGET_RETRIES: u8 = 2;

/// Manages window transaction IDs and their associated target frames.
#[derive(Debug)]
pub struct TransactionManager {
    pub store: WindowTxStore,
    rejected_targets: DashMap<WindowServerId, (CGRect, u8)>,
}

impl TransactionManager {
    pub fn new(store: WindowTxStore) -> Self {
        Self {
            store,
            rejected_targets: Default::default(),
        }
    }

    /// Stores a transaction ID for a window with its target frame.
    pub fn store_txid(&self, wsid: WindowServerId, txid: TransactionId, target: CGRect) {
        self.store.insert(wsid, txid, target);
    }

    /// Updates multiple transaction ID entries.
    pub fn update_txid_entries<I>(&self, entries: I)
    where I: IntoIterator<Item = (WindowServerId, TransactionId, CGRect)> {
        for (wsid, txid, target) in entries {
            self.store.insert(wsid, txid, target);
        }
    }

    /// Removes the transaction ID entry for a window.
    pub fn remove_for_window(&self, wsid: WindowServerId) {
        self.store.remove(&wsid);
        self.rejected_targets.remove(&wsid);
    }

    /// Clears the pending target for a window while preserving its last txid.
    pub fn clear_target_for_window(&self, wsid: WindowServerId) { self.store.clear_target(&wsid); }

    /// Generates the next transaction ID for a window.
    pub fn generate_next_txid(&self, wsid: WindowServerId) -> TransactionId {
        self.store.next_txid(wsid)
    }

    /// Sets the last sent transaction ID for a window.
    pub fn set_last_sent_txid(&self, wsid: WindowServerId, txid: TransactionId) {
        self.store.set_last_txid(wsid, txid);
    }

    /// Gets the last sent transaction ID for a window.
    pub fn get_last_sent_txid(&self, wsid: WindowServerId) -> TransactionId {
        self.store.last_txid(&wsid)
    }

    /// Gets the target frame for a window's transaction, if it exists.
    pub fn get_target_frame(&self, wsid: WindowServerId) -> Option<CGRect> {
        self.store.get(&wsid)?.target
    }

    /// Records that the app answered `target` with another frame; returns whether the
    /// target may be re-sent (bounded per window and target).
    pub fn retry_rejected_target(&self, wsid: WindowServerId, target: CGRect) -> bool {
        let mut entry = self.rejected_targets.entry(wsid).or_insert((target, 0));
        if !entry.0.same_as(target) {
            *entry = (target, 0);
        }
        if entry.1 >= MAX_REJECTED_TARGET_RETRIES {
            return false;
        }
        entry.1 += 1;
        true
    }

    /// Forgets the rejected-target retry count for a window.
    pub fn forget_rejected_target(&self, wsid: WindowServerId) {
        self.rejected_targets.remove(&wsid);
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGSize};

    use super::*;

    fn rect(x: f64) -> CGRect { CGRect::new(CGPoint::new(x, 0.), CGSize::new(100., 100.)) }

    fn spend_budget(manager: &TransactionManager, wsid: WindowServerId, target: CGRect) {
        for _ in 0..MAX_REJECTED_TARGET_RETRIES {
            assert!(manager.retry_rejected_target(wsid, target));
        }
        assert!(!manager.retry_rejected_target(wsid, target));
    }

    #[test]
    fn rejected_target_budget_resets_for_a_new_target() {
        let manager = TransactionManager::new(WindowTxStore::new());
        let wsid = WindowServerId::new(1);
        spend_budget(&manager, wsid, rect(0.));
        spend_budget(&manager, wsid, rect(10.));
        assert!(manager.retry_rejected_target(WindowServerId::new(2), rect(10.)));
    }

    #[test]
    fn rejected_target_budget_resets_when_forgotten_or_removed() {
        let manager = TransactionManager::new(WindowTxStore::new());
        let wsid = WindowServerId::new(1);
        spend_budget(&manager, wsid, rect(0.));
        manager.forget_rejected_target(wsid);
        spend_budget(&manager, wsid, rect(0.));
        manager.remove_for_window(wsid);
        spend_budget(&manager, wsid, rect(0.));
    }
}
