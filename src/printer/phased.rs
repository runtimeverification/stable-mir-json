//! Typestate wrapper encoding the post-monomorphization body invariant.
//!
//! After the collection pipeline's fixpoint loop resolves all unevaluated
//! constants, bodies stored in [`Item`](super::schema::Item) should contain
//! no `ConstantKind::Unevaluated`. [`Phased<T, S>`] encodes this guarantee
//! in the type system: phase 1+2 code works with `Phased<Body, Raw>`, and
//! the phase 2 -> 3 boundary validates each body (via [`UnevaluatedChecker`])
//! before producing `Phased<Body, Mono>`.
//!
//! [`Phased`] serializes transparently (delegates to the inner `T`), so
//! JSON output is unchanged.

use std::marker::PhantomData;

use crate::compat::serde;
use crate::compat::stable_mir;

use serde::{Serialize, Serializer};
use stable_mir::mir::visit::MirVisitor;
use stable_mir::mir::Body;

/// Pre-validation phase marker. Bodies may still contain `ConstantKind::Unevaluated`.
pub struct Raw;

/// Post-validation phase marker. Bodies are guaranteed free of `ConstantKind::Unevaluated`.
pub struct Mono;

/// A value of type `T` tagged with a phase marker `S`.
///
/// Construction of `Phased<T, Raw>` is unrestricted; `Phased<T, Mono>` can
/// only be obtained through [`Phased::<Body, Raw>::validate`], which walks
/// the body and panics if any unevaluated constant is found.
pub struct Phased<T, S>(T, PhantomData<S>);

impl<T, S> Phased<T, S> {
    /// Borrow the inner value.
    pub fn inner(&self) -> &T {
        &self.0
    }

    /// Consume the wrapper, returning the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Clone, S> Clone for Phased<T, S> {
    fn clone(&self) -> Self {
        Phased(self.0.clone(), PhantomData)
    }
}

impl<T: Serialize, S> Serialize for Phased<T, S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl Phased<Body, Raw> {
    /// Wrap a body in the `Raw` phase. Unrestricted.
    pub fn new(body: Body) -> Self {
        Phased(body, PhantomData)
    }

    /// Validate that the body contains no `ConstantKind::Unevaluated`, then
    /// promote to the `Mono` phase. Panics if the invariant is violated.
    pub fn validate(self) -> Phased<Body, Mono> {
        UnevaluatedChecker.visit_body(&self.0);
        Phased(self.0, PhantomData)
    }
}

/// MIR visitor that panics if any `ConstantKind::Unevaluated` is encountered.
///
/// Used by [`Phased::<Body, Raw>::validate`] to enforce the post-mono invariant
/// at the phase 2 -> 3 boundary.
struct UnevaluatedChecker;

impl MirVisitor for UnevaluatedChecker {
    fn visit_mir_const(
        &mut self,
        constant: &stable_mir::ty::MirConst,
        loc: stable_mir::mir::visit::Location,
    ) {
        if let stable_mir::ty::ConstantKind::Unevaluated(_) = constant.kind() {
            panic!(
                "Phased<Body, Raw>::validate: body contains ConstantKind::Unevaluated \
                 at location {loc:?}; all unevaluated constants should have been \
                 resolved by the fixpoint loop before phase 3"
            );
        }
        self.super_mir_const(constant, loc);
    }
}
