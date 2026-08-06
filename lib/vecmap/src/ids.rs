/// Generates strongly-typed ID newtypes and implements allocation logic.
///
/// Each generated ID type is backed by a `usize` and can be used
/// as an index into `VecMap`.
#[macro_export]
macro_rules! impl_vecmap_ids {
    ( $( $( #[$meta:meta] )* $ty:ident ),* $(,)? ) => {
        $(
            $( #[$meta] )*
            #[derive(
                Debug, Clone, Copy, Hash,
                PartialEq, Eq,
                PartialOrd, Ord,
                serde::Serialize, serde::Deserialize
            )]
            pub struct $ty(usize);

            impl $ty {
                pub const ERROR: $ty = $ty(0);
            }

            impl $crate::VecMapIndex for $ty {
                fn new_index(value: usize) -> Self {
                    Self(value)
                }

                fn index(&self) -> usize {
                    self.0
                }
            }

            impl $crate::IdAlloc for $ty {

                fn error() -> Self {
                    Self(0)
                }

                fn begin() -> Self {
                    Self(1)
                }

                fn alloc(&mut self) -> Self {
                    let new = *self;
                    self.0 += 1;
                    new
                }

                fn last(&self) -> Self {
                    self.clone()
                }
            }
        )*
    };
}

/// used for internal idGenerator
pub trait IdAlloc: Clone {
    /// unique error Id always the same
    fn error() -> Self;

    /// Returns the initial value for the ID generator.
    fn begin() -> Self;

    /// Allocates a new unique ID and advances the generator.
    fn alloc(&mut self) -> Self;

    /// get last allocated ID.
    fn last(&self) -> Self;
}

/// Generates unique IDs for a given context.
///
/// Each generator instance produces a monotonically increasing
/// sequence of IDs of a single type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IdGenerator<Id: IdAlloc> {
    current: Id,
}
impl<Id: IdAlloc> IdGenerator<Id> {
    /// Creates a new ID generator.
    pub fn new() -> Self {
        Self {
            current: Id::begin(),
        }
    }

    /// Allocates and returns a fresh ID.
    pub fn alloc(&mut self) -> Id {
        self.current.alloc()
    }

    pub fn last(&self) -> Id {
        self.current.last()
    }
}
impl<Id: IdAlloc> Default for IdGenerator<Id> {
    fn default() -> Self {
        Self {
            current: Id::begin(),
        }
    }
}
