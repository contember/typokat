//! Source identity types shared across architecture layers.

/// A module's position in the original driver input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ModuleOrdinal(usize);

impl ModuleOrdinal {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// A module's dependency-ordered slot in the checker.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct UnitSlot(usize);

impl UnitSlot {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// A user module's position in the original driver input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OriginalModuleOrdinal(usize);

impl OriginalModuleOrdinal {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// A library file's position in the pinned default-library profile.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibraryFileOrdinal(usize);

impl LibraryFileOrdinal {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

/// Stable compilation ownership retained across dependency ordering.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum CompilationOrigin {
    User(OriginalModuleOrdinal),
    Library(LibraryFileOrdinal),
}

/// Stable source ordering domain used by checker-local indexes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SourceOrdinal {
    User(ModuleOrdinal),
    Library(LibraryFileOrdinal),
}

/// Exact source unit retained by checker producers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SourceUnit {
    User {
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
    },
    Library {
        file_ordinal: LibraryFileOrdinal,
    },
}
