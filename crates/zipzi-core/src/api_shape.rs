//! Small, language-neutral descriptions of explicitly selected authoring types.
//! These macros emit the real Rust records and their shape from one declaration;
//! they do not implement validation, defaults beyond serde, or domain behavior.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    String,
    U32,
    F32,
    Bool,
    U64,
    Usize,
    Bytes,
    StringMap {
        value: Box<Shape>,
    },
    Optional {
        item: Box<Shape>,
    },
    Sequence {
        item: Box<Shape>,
    },
    Array {
        item: Box<Shape>,
        length: usize,
    },
    Record {
        name: String,
        fields: Vec<Field>,
    },
    Union {
        name: String,
        tag: String,
        variants: Vec<Variant>,
    },
    Opaque {
        name: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    pub name: String,
    pub shape: Shape,
    pub default: Option<DefaultValue>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DefaultValue {
    Null,
    EmptyString,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Variant {
    pub rust_name: String,
    pub wire_name: String,
    pub fields: Vec<Field>,
}

pub trait ApiShape {
    fn api_shape() -> Shape;
}
pub trait ApiDefault {
    fn api_default() -> DefaultValue;
}
impl ApiShape for String {
    fn api_shape() -> Shape {
        Shape::String
    }
}
impl ApiDefault for String {
    fn api_default() -> DefaultValue {
        DefaultValue::EmptyString
    }
}
impl ApiShape for u32 {
    fn api_shape() -> Shape {
        Shape::U32
    }
}
impl ApiShape for f32 {
    fn api_shape() -> Shape {
        Shape::F32
    }
}
impl ApiShape for bool {
    fn api_shape() -> Shape {
        Shape::Bool
    }
}
impl ApiShape for u64 {
    fn api_shape() -> Shape {
        Shape::U64
    }
}
impl ApiShape for usize {
    fn api_shape() -> Shape {
        Shape::Usize
    }
}
impl ApiShape for std::sync::Arc<[u8]> {
    fn api_shape() -> Shape {
        Shape::Bytes
    }
}
impl ApiShape for std::collections::BTreeMap<String, String> {
    fn api_shape() -> Shape {
        Shape::StringMap {
            value: Box::new(Shape::String),
        }
    }
}
impl<T: ApiShape> ApiShape for Option<T> {
    fn api_shape() -> Shape {
        Shape::Optional {
            item: Box::new(T::api_shape()),
        }
    }
}
impl<T> ApiDefault for Option<T> {
    fn api_default() -> DefaultValue {
        DefaultValue::Null
    }
}
impl<T: ApiShape> ApiShape for Vec<T> {
    fn api_shape() -> Shape {
        Shape::Sequence {
            item: Box::new(T::api_shape()),
        }
    }
}
impl<T: ApiShape, const N: usize> ApiShape for [T; N] {
    fn api_shape() -> Shape {
        Shape::Array {
            item: Box::new(T::api_shape()),
            length: N,
        }
    }
}
impl Field {
    pub fn of<T: ApiShape>(name: &str) -> Self {
        let shape = T::api_shape();
        let default = matches!(shape, Shape::Optional { .. }).then_some(DefaultValue::Null);
        Self {
            name: name.into(),
            shape,
            default,
        }
    }
    pub fn defaulted<T: ApiShape + ApiDefault>(name: &str) -> Self {
        Self {
            name: name.into(),
            shape: T::api_shape(),
            default: Some(T::api_default()),
        }
    }
}
pub fn snake_case(name: &str) -> String {
    let mut result = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && index != 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

#[doc(hidden)]
#[macro_export]
macro_rules! api_record_attributes {
    () => { () };
    (#[derive($($traits:tt)*)] $($rest:tt)*) => { $crate::api_record_attributes!($($rest)*) };
    (#[serde(deny_unknown_fields)] $($rest:tt)*) => { $crate::api_record_attributes!($($rest)*) };
    (#[doc = $doc:literal] $($rest:tt)*) => { $crate::api_record_attributes!($($rest)*) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! api_field {
    ($name:ident: $ty:ty; #[serde(default)] $($rest:tt)*) => { $crate::api_field!(@default $name: $ty; $($rest)*) };
    (@default $name:ident: $ty:ty; #[doc = $doc:literal] $($rest:tt)*) => { $crate::api_field!(@default $name: $ty; $($rest)*) };
    (@default $name:ident: $ty:ty;) => { $crate::api_shape::Field::defaulted::<$ty>(stringify!($name)) };
    ($name:ident: $ty:ty; #[doc = $doc:literal] $($rest:tt)*) => { $crate::api_field!($name: $ty; $($rest)*) };
    ($name:ident: $ty:ty;) => { $crate::api_shape::Field::of::<$ty>(stringify!($name)) };
}
#[macro_export]
macro_rules! api_record {
    ($(#[$($meta:tt)*])* pub struct $name:ident { $($(#[$($field_meta:tt)*])* pub $field:ident: $ty:ty),* $(,)? }) => {
        const _: () = $crate::api_record_attributes!($(#[$($meta)*])*);
        $(#[$($meta)*])* pub struct $name { $($(#[$($field_meta)*])* pub $field: $ty),* }
        impl $crate::api_shape::ApiShape for $name {
            fn api_shape() -> $crate::api_shape::Shape {
                $crate::api_shape::Shape::Record {
                    name: concat!(module_path!(), "::", stringify!($name)).into(),
                    fields: vec![$($crate::api_field!($field: $ty; $(#[$($field_meta)*])*)),*],
                }
            }
        }
    };
}
#[macro_export]
macro_rules! api_union {
    (tag = $tag:literal; $(#[$($meta:tt)*])* pub enum $name:ident { $($variant:ident { $($field:ident: $ty:ty),* $(,)? }),* $(,)? }) => {
        const _: () = $crate::api_record_attributes!($(#[$($meta)*])*);
        $(#[$($meta)*])* #[serde(tag = $tag, rename_all = "snake_case", deny_unknown_fields)]
        pub enum $name { $($variant { $($field: $ty),* }),* }
        impl $crate::api_shape::ApiShape for $name {
            fn api_shape() -> $crate::api_shape::Shape {
                $crate::api_shape::Shape::Union {
                    name: concat!(module_path!(), "::", stringify!($name)).into(), tag: $tag.into(),
                    variants: vec![$($crate::api_shape::Variant {
                        rust_name: stringify!($variant).into(), wire_name: $crate::api_shape::snake_case(stringify!($variant)),
                        fields: vec![$($crate::api_shape::Field::of::<$ty>(stringify!($field))),*],
                    }),*],
                }
            }
        }
    };
}
