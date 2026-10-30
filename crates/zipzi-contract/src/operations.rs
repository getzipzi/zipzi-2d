//! Operation identities and typed inputs/outputs shared with execution bindings.
use crate::api_shape::{ApiShape, Shape};
use std::marker::PhantomData;

pub struct Operation<I, O> {
    pub name: &'static str,
    types: PhantomData<fn(I) -> O>,
}
impl<I, O> Operation<I, O> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            types: PhantomData,
        }
    }
}
impl<I: ApiShape, O: ApiShape> Operation<I, O> {
    pub fn shapes(&self) -> (Shape, Shape) {
        (I::api_shape(), O::api_shape())
    }
}

/// The same list declares contract operations and requires Runtime bindings.
#[macro_export]
macro_rules! authoring_operations {
    ($callback:ident) => {
        $callback! {
            MESH_QUERY: $crate::runtime::MeshQuery => $crate::runtime::MeshSnapshot = "mesh_snapshot";
            IMAGE_QUERY: $crate::runtime::ImageQuery => $crate::runtime::ImageSnapshot = "image_snapshot";
            PREPARED_PIXELS_QUERY: $crate::runtime::PreparedPixelsQuery => $crate::runtime::ImagePixels = "prepared_pixels";
            PLAN_QUERY: $crate::runtime::PlanQuery => $crate::runtime::PlanSnapshot = "plan_authoring";
        }
    };
}

const fn equal(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}
#[doc(hidden)]
pub const fn assert_unique(names: &[&str]) {
    let mut i = 0;
    while i < names.len() {
        let mut j = i + 1;
        while j < names.len() {
            assert!(!equal(names[i], names[j]), "duplicate operation name");
            j += 1;
        }
        i += 1;
    }
}
macro_rules! declarations {
    ($($id:ident: $input:ty => $output:ty = $name:literal;)*) => {
        $(pub const $id: Operation<$input, $output> = Operation::new($name);)*
        const _: () = assert_unique(&[$($name),*]);
    };
}
crate::authoring_operations!(declarations);
