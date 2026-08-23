macro_rules! dense_key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub(crate) fn new(index: usize) -> Option<Self> {
                u32::try_from(index).ok().map(Self)
            }

            pub fn index(self) -> usize {
                usize::try_from(self.0).unwrap_or(usize::MAX)
            }
        }
    };
}

dense_key!(SourceKey);
dense_key!(ContainerKey);
dense_key!(RegionKey);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallableScopeKey(u32);

impl CallableScopeKey {
    pub(crate) fn new(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    pub fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}
dense_key!(DeclarationKey);
dense_key!(GroupKey);
dense_key!(NamespaceKey);
dense_key!(ValueKey);
dense_key!(TypeParameterKey);
dense_key!(TypeRecipeKey);
dense_key!(SignatureKey);
dense_key!(MemberKey);
dense_key!(StringKey);
