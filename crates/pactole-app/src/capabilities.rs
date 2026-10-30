use thiserror::Error;

/// A feature exposed by a ledger backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// The backend can provide a readable ledger snapshot.
    Read,
    /// The backend can persist changes.
    Write,
    /// The backend supports transaction write operations.
    Transactions,
    /// The backend supports budget operations.
    Budgets,
}

/// Capabilities advertised by a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// Whether the backend can read snapshots.
    read: bool,
    /// Whether the backend can persist changes.
    write: bool,
    /// Whether transaction writes are supported.
    transactions: bool,
    /// Whether budget operations are supported.
    budgets: bool,
}

/// Invalid combination of backend capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CapabilityError {
    /// A write-specific capability was enabled without write support.
    #[error("{capability:?} capability requires write support")]
    RequiresWrite {
        /// Capability that requires write support.
        capability: Capability,
    },
}

impl BackendCapabilities {
    /// Creates capabilities for a read-only backend.
    pub const fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            transactions: false,
            budgets: false,
        }
    }

    /// Creates a capability set after checking its invariants.
    pub const fn new(
        read: bool,
        write: bool,
        transactions: bool,
        budgets: bool,
    ) -> Result<Self, CapabilityError> {
        if (transactions || budgets) && !write {
            return Err(CapabilityError::RequiresWrite {
                capability: if transactions {
                    Capability::Transactions
                } else {
                    Capability::Budgets
                },
            });
        }
        Ok(Self {
            read,
            write,
            transactions,
            budgets,
        })
    }

    /// Returns whether `capability` is supported.
    pub const fn supports(self, capability: Capability) -> bool {
        match capability {
            Capability::Read => self.read,
            Capability::Write => self.write,
            Capability::Transactions => self.transactions,
            Capability::Budgets => self.budgets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_capabilities_only_support_reading() {
        let capabilities = BackendCapabilities::read_only();
        assert!(capabilities.supports(Capability::Read));
        assert!(!capabilities.supports(Capability::Write));
        assert!(!capabilities.supports(Capability::Transactions));
        assert!(!capabilities.supports(Capability::Budgets));
    }

    #[test]
    fn write_capabilities_require_write_support() {
        assert!(BackendCapabilities::new(true, false, true, false).is_err());
        assert!(BackendCapabilities::new(true, false, false, true).is_err());
        assert!(BackendCapabilities::new(true, true, true, true).is_ok());
    }
}
