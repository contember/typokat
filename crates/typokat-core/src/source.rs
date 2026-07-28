//! Source identity types shared across architecture layers.

/// A module's position in the original driver input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleOrdinal(usize);

impl ModuleOrdinal {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

/// A module's dependency-ordered slot in the checker.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitSlot(usize);

impl UnitSlot {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

/// A user module's position in the original driver input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OriginalModuleOrdinal(usize);

impl OriginalModuleOrdinal {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

/// A library file's position in the pinned default-library profile.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibraryFileOrdinal(usize);

impl LibraryFileOrdinal {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

/// Stable compilation ownership retained across dependency ordering.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompilationOrigin {
    User(OriginalModuleOrdinal),
    Library(LibraryFileOrdinal),
}

/// Stable source ordering domain used by checker-local indexes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceOrdinal {
    User(ModuleOrdinal),
    Library(LibraryFileOrdinal),
}

/// Exact source unit retained by checker producers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceUnit {
    User {
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
    },
    Library {
        file_ordinal: LibraryFileOrdinal,
    },
}
