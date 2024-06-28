//! Interned types

/// Type of interner
pub type Interner = lasso::ThreadedRodeo;

macro_rules! intern_newtype {
    ($name:ident) => {
        /// Newtype for interning
        ///
        /// Treat this as an opaque token
        #[repr(transparent)]
        #[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
        pub struct $name(pub(crate) lasso::Spur);

        impl $name {
            /// Intern or get an existing interned string
            #[inline]
            pub fn get_or_intern<T>(interner: &Interner, val: T) -> Self
            where
                T: AsRef<str>,
            {
                Self(interner.get_or_intern(val))
            }

            /// Create a new instance wrapping an interning token
            #[inline]
            pub fn new(spur: lasso::Spur) -> Self {
                Self(spur)
            }

            /// Get a type suitable for use with the interner
            ///
            /// Specific type is not stable and public (i.e. what interner is used can change).
            #[inline]
            pub fn as_interner_ref(&self) -> lasso::Spur {
                self.0
            }

            /// Convert to a string
            #[inline]
            pub fn to_str<'interner>(&self, interner: &'interner Interner) -> &'interner str {
                interner.resolve(&self.as_interner_ref())
            }

            /// Convert to a string
            #[inline]
            pub fn try_to_str<'interner>(
                &self,
                interner: &'interner Interner,
            ) -> Option<&'interner str> {
                interner.try_resolve(&self.as_interner_ref())
            }
        }
    };
}

intern_newtype!(PackageRef);
intern_newtype!(ArchitectureRef);
