//! Small, language-neutral descriptions of explicitly selected authoring types.
//! These macros emit the real Rust records and their shape from one declaration;
//! they do not implement validation, defaults beyond serde, or domain behavior.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    String,
    U8,
    U16,
    I64,
    F64,
    Json,
    Shared {
        item: Box<Shape>,
    },
    IndexMap {
        key: Box<Shape>,
        value: Box<Shape>,
    },
    U32,
    I32,
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
    SerdeRecord {
        name: String,
        fields: Vec<SerdeField>,
        transparent: bool,
        tuple: bool,
    },
    SerdeEnum {
        name: String,
        tag: Option<String>,
        content: Option<String>,
        variants: Vec<SerdeVariant>,
    },
    Opaque {
        name: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    pub doc: String,
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
impl ApiShape for i32 {
    fn api_shape() -> Shape {
        Shape::I32
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
impl<T: ApiShape> ApiShape for std::sync::Arc<[T]> {
    fn api_shape() -> Shape {
        let item = T::api_shape();
        if matches!(item, Shape::U8) {
            Shape::Bytes
        } else {
            Shape::Shared {
                item: Box::new(Shape::Sequence {
                    item: Box::new(item),
                }),
            }
        }
    }
}
impl<T: ApiShape> ApiShape for std::collections::BTreeMap<String, T> {
    fn api_shape() -> Shape {
        Shape::StringMap {
            value: Box::new(T::api_shape()),
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
    pub fn documented(mut self, doc: &str) -> Self {
        self.doc = if self.doc.is_empty() {
            doc.trim().into()
        } else {
            format!("{}\n{}", doc.trim(), self.doc)
        };
        self
    }

    pub fn of<T: ApiShape>(name: &str) -> Self {
        let shape = T::api_shape();
        let default = matches!(shape, Shape::Optional { .. }).then_some(DefaultValue::Null);
        Self {
            doc: String::new(),
            name: name.into(),
            shape,
            default,
        }
    }
    pub fn defaulted<T: ApiShape + ApiDefault>(name: &str) -> Self {
        Self {
            doc: String::new(),
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
    (@default $name:ident: $ty:ty; #[doc = $doc:literal] $($rest:tt)*) => { $crate::api_field!(@default $name: $ty; $($rest)*).documented($doc) };
    (@default $name:ident: $ty:ty;) => { $crate::api_shape::Field::defaulted::<$ty>(stringify!($name)) };
    ($name:ident: $ty:ty; #[doc = $doc:literal] $($rest:tt)*) => { $crate::api_field!($name: $ty; $($rest)*).documented($doc) };
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SerdeField {
    pub doc: String,
    pub rust_name: String,
    pub name: String,
    pub shape: Shape,
    pub skip: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SerdeVariant {
    pub rust_name: String,
    pub name: String,
    pub form: String,
    pub fields: Vec<SerdeField>,
}
impl ApiShape for u8 {
    fn api_shape() -> Shape {
        Shape::U8
    }
}
impl ApiShape for u16 {
    fn api_shape() -> Shape {
        Shape::U16
    }
}
impl ApiShape for i64 {
    fn api_shape() -> Shape {
        Shape::I64
    }
}
impl ApiShape for f64 {
    fn api_shape() -> Shape {
        Shape::F64
    }
}
impl ApiShape for serde_json::Value {
    fn api_shape() -> Shape {
        Shape::Json
    }
}
impl ApiShape for std::path::PathBuf {
    fn api_shape() -> Shape {
        Shape::String
    }
}

impl<T: ApiShape> ApiShape for std::sync::Arc<T> {
    fn api_shape() -> Shape {
        Shape::Shared {
            item: Box::new(T::api_shape()),
        }
    }
}
impl<T: ApiShape> ApiShape for std::collections::BTreeMap<usize, T> {
    fn api_shape() -> Shape {
        Shape::IndexMap {
            key: Box::new(Shape::Usize),
            value: Box::new(T::api_shape()),
        }
    }
}

impl<T: ApiShape> ApiShape for std::collections::HashMap<String, T> {
    fn api_shape() -> Shape {
        Shape::StringMap {
            value: Box::new(T::api_shape()),
        }
    }
}
impl<T: ApiShape> ApiShape for std::collections::HashMap<u32, T> {
    fn api_shape() -> Shape {
        Shape::IndexMap {
            key: Box::new(Shape::U32),
            value: Box::new(T::api_shape()),
        }
    }
}

pub fn serde_default<T: serde::Serialize>(value: T) -> Option<serde_json::Value> {
    Some(serde_json::to_value(value).expect("selected serde default"))
}

impl ApiShape for &str {
    fn api_shape() -> Shape {
        Shape::String
    }
}
