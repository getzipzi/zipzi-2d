//! Mechanical field descriptions. The exporter selects the finite public roots;
//! domain validation and behavior remain in core/SDK.
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Fields};
#[derive(Default)]
struct Serde {
    rename: Option<String>,
    rename_all: Option<String>,
    tag: Option<String>,
    content: Option<String>,
    transparent: bool,
    skip: bool,
    default: Option<Option<String>>,
}
fn attrs(attrs: &[syn::Attribute]) -> syn::Result<Serde> {
    let mut result = Serde::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(|m| {
            let dest = if m.path.is_ident("rename") {
                Some(&mut result.rename)
            } else if m.path.is_ident("rename_all") {
                Some(&mut result.rename_all)
            } else if m.path.is_ident("tag") {
                Some(&mut result.tag)
            } else if m.path.is_ident("content") {
                Some(&mut result.content)
            } else {
                None
            };
            if let Some(dest) = dest {
                *dest = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else if m.path.is_ident("default") {
                result.default = Some(if m.input.peek(syn::Token![=]) {
                    Some(m.value()?.parse::<syn::LitStr>()?.value())
                } else {
                    None
                });
            } else if m.path.is_ident("transparent") {
                result.transparent = true;
            } else if m.path.is_ident("skip") {
                result.skip = true;
            } else if m.path.is_ident("deny_unknown_fields") {
            } else if m.path.is_ident("skip_serializing_if") {
                let _: syn::LitStr = m.value()?.parse()?;
            } else if m.path.is_ident("deserialize_with") {
                let path = m.value()?.parse::<syn::LitStr>()?;
                if !matches!(
                    path.value().as_str(),
                    "crate::serde_maps::usize_map" | "zipzi_core::serde_maps::u32_map"
                ) {
                    return Err(m.error("Unreviewed selected field deserializer"));
                }
            } else {
                return Err(m.error("Unsupported selected serde attribute"));
            }

            Ok(())
        })?;
    }
    Ok(result)
}
fn wire(name: &str, rename: Option<&str>) -> syn::Result<String> {
    Ok(match rename {
        None => name.into(),
        Some("snake_case") => {
            let mut out = String::new();
            for (i, c) in name.chars().enumerate() {
                if i > 0 && c.is_ascii_uppercase() {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            }
            out
        }
        Some("camelCase") => {
            let mut out = String::new();
            let mut upper = false;
            for c in name.chars() {
                if c == '_' {
                    upper = true;
                } else if upper {
                    out.push(c.to_ascii_uppercase());
                    upper = false;
                } else {
                    out.push(c);
                }
            }
            out
        }
        Some("lowercase") => name.to_ascii_lowercase(),
        Some("PascalCase") => name.into(),
        Some(other) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("Unsupported shape rename: {other}"),
            ));
        }
    })
}
fn doc_text(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|a| {
            if !a.path().is_ident("doc") {
                return None;
            }
            if let syn::Meta::NameValue(v) = &a.meta {
                if let syn::Expr::Lit(v) = &v.value {
                    if let syn::Lit::Str(v) = &v.lit {
                        return Some(v.value().trim().to_owned());
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn fields(
    fields: &Fields,
    rename_all: Option<&str>,
    inherited_default: bool,
) -> syn::Result<proc_macro2::TokenStream> {
    let values = fields.iter().enumerate().map(|(i,f)| {
        let a=attrs(&f.attrs)?;
        let doc=doc_text(&f.attrs);
        let rust=f.ident.as_ref().map(ToString::to_string).unwrap_or_else(||i.to_string());
        let name=if let Some(n)=a.rename { n } else if f.ident.is_none() { if fields.len()==1 { "value".into() } else { format!("value{i}") } } else { wire(&rust, rename_all)? };
        let ty=&f.ty;
        let shape=if a.skip { quote!(zipzi_core::api_shape::Shape::Opaque {name: "__skipped".into()}) } else { quote!(<#ty as zipzi_core::api_shape::ApiShape>::api_shape()) };
        let skip=a.skip;
        let option=matches!(ty, syn::Type::Path(p) if p.path.segments.last().is_some_and(|s|s.ident=="Option"));
        let default=if skip {quote!(None)} else if let Some(default)=a.default.or(inherited_default.then_some(None)).or(option.then_some(None)) {
            let expr=if let Some(path)=default {let path:syn::Path=syn::parse_str(&path)?;quote!(#path())} else {quote!(<#ty as Default>::default())};
            quote!(zipzi_core::api_shape::serde_default(#expr))
        } else {quote!(None)};
        Ok(quote!(zipzi_core::api_shape::SerdeField {doc:#doc.into(),rust_name:#rust.into(), name:#name.into(), shape:#shape, skip:#skip, default:#default}))
    }).collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(vec![#(#values),*]))
}
/// Select field names and concrete Rust type identities without recursively
/// exporting private lifecycle state. DTO exporters bind these finite leaves.
#[proc_macro_derive(ApiFields)]
pub fn api_fields(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let field_rows = |fields: &Fields| {
        fields.iter().enumerate().map(|(index, field)| {
            let key = field.ident.as_ref().map(ToString::to_string)
                .unwrap_or_else(|| if fields.len() == 1 { "value".into() } else { format!("value{index}") });
            let ty = &field.ty;
            let doc=doc_text(&field.attrs);
            quote!(zipzi_core::api_shape::SerdeField {
                doc:#doc.into(),
                rust_name: #key.into(), name: #key.into(),
                shape: zipzi_core::api_shape::Shape::Opaque { name: std::any::type_name::<#ty>().into() },
                skip: false, default: None,
            })
        }).collect::<Vec<_>>()
    };
    let shape = match &input.data {
        syn::Data::Struct(data) => {
            let rows = field_rows(&data.fields);
            quote!(zipzi_core::api_shape::Shape::SerdeRecord {
                name: std::any::type_name::<Self>().into(),
                fields: vec![#(#rows),*],
                transparent: false,
                tuple: false,
            })
        }
        syn::Data::Enum(data) => {
            let rows = data.variants.iter().map(|variant| {
                let rust_name = variant.ident.to_string();
                let rows = field_rows(&variant.fields);
                let form = match &variant.fields {
                    Fields::Named(_) => "record", Fields::Unit => "unit", Fields::Unnamed(_) => "newtype",
                };
                quote!(zipzi_core::api_shape::SerdeVariant {
                    rust_name: #rust_name.into(), name: #rust_name.into(), form: #form.into(), fields: vec![#(#rows),*],
                })
            });
            quote!(zipzi_core::api_shape::Shape::SerdeEnum {
                name: std::any::type_name::<Self>().into(),
                tag: None,
                content: None,
                variants: vec![#(#rows),*],
            })
        }
        _ => {
            return syn::Error::new_spanned(name, "ApiFields requires a struct or enum")
                .to_compile_error()
                .into();
        }
    };
    quote!(impl #name {
        pub fn api_fields() -> zipzi_core::api_shape::Shape { #shape }
    })
    .into()
}

#[proc_macro_derive(ApiShape, attributes(serde))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(t) => t.into(),
        Err(e) => e.into_compile_error().into(),
    }
}
fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let a = attrs(&input.attrs)?;

    let mut generics = input.generics.clone();
    let params = generics
        .type_params()
        .map(|p| p.ident.clone())
        .collect::<Vec<_>>();
    for param in params {
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#param:zipzi_core::api_shape::ApiShape));
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let body = match input.data {
        syn::Data::Struct(data) => {
            let fs = fields(&data.fields, a.rename_all.as_deref(), a.default.is_some())?;
            let transparent = a.transparent;
            let tuple = matches!(data.fields, Fields::Unnamed(_));
            quote!(zipzi_core::api_shape::Shape::SerdeRecord {name:std::any::type_name::<Self>().into(),fields:#fs,transparent:#transparent,tuple:#tuple})
        }
        syn::Data::Enum(data) => {
            let mut vs = vec![];
            for v in data.variants {
                let va = attrs(&v.attrs)?;
                if va.skip {
                    continue;
                }
                let rn = v.ident.to_string();
                let wn = if let Some(n) = va.rename {
                    n
                } else {
                    wire(&rn, a.rename_all.as_deref())?
                };
                let form = match &v.fields {
                    Fields::Unit => "unit",
                    Fields::Named(_) => "record",
                    Fields::Unnamed(f) if f.unnamed.len() == 1 => "newtype",
                    Fields::Unnamed(_) => "tuple",
                };
                let fs = fields(&v.fields, None, false)?;
                vs.push(quote!(zipzi_core::api_shape::SerdeVariant {rust_name:#rn.into(),name:#wn.into(),form:#form.into(),fields:#fs}));
            }
            let tag = a
                .tag
                .map(|v| quote!(Some(#v.into())))
                .unwrap_or(quote!(None));
            let content = a
                .content
                .map(|v| quote!(Some(#v.into())))
                .unwrap_or(quote!(None));
            quote!(zipzi_core::api_shape::Shape::SerdeEnum {name:std::any::type_name::<Self>().into(),tag:#tag,content:#content,variants:vec![#(#vs),*]})
        }
        _ => return Err(syn::Error::new_spanned(name, "Unions are not API shapes")),
    };
    Ok(
        quote!(impl #impl_generics zipzi_core::api_shape::ApiShape for #name #ty_generics #where_clause {fn api_shape()->zipzi_core::api_shape::Shape {#body}}),
    )
}

/// Binary layout is finite and compile-time. Serde's JSON tagging does not add
/// intermediate Content/Value trees to this transport.
#[proc_macro_derive(BinaryWire, attributes(serde))]
pub fn binary_wire(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match binary_expand(input, true) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
#[proc_macro_derive(BinaryOutput, attributes(serde))]
pub fn binary_output(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match binary_expand(input, false) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
fn binary_skip(attrs: &[syn::Attribute]) -> syn::Result<bool> {
    let mut skip = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        let meta = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        )?;
        for value in meta {
            if value.path().is_ident("skip") {
                skip = true;
            }
        }
    }
    Ok(skip)
}
fn binary_expand(input: DeriveInput, read: bool) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let base = quote!(zipzi_core::binary_wire);
    let mut write_generics = input.generics.clone();
    let mut read_generics = input.generics.clone();
    let params = input
        .generics
        .type_params()
        .map(|p| p.ident.clone())
        .collect::<Vec<_>>();
    for param in params {
        write_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#param:zipzi_core::binary_wire::WireWrite));
        read_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#param:zipzi_core::binary_wire::WireRead));
    }
    let (wi, wt, ww) = write_generics.split_for_impl();
    let (ri, rt, rw) = read_generics.split_for_impl();
    struct Field {
        member: syn::Member,
        ty: syn::Type,
        skip: bool,
        variable: syn::Ident,
    }
    fn selected(fields: &Fields) -> syn::Result<Vec<Field>> {
        fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                Ok(Field {
                    member: f
                        .ident
                        .clone()
                        .map(syn::Member::Named)
                        .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(i))),
                    ty: f.ty.clone(),
                    skip: binary_skip(&f.attrs)?,
                    variable: syn::Ident::new(
                        &format!("field_{i}"),
                        proc_macro2::Span::call_site(),
                    ),
                })
            })
            .collect()
    }
    let field_signature = |mut seed: proc_macro2::TokenStream, fields: &[Field]| {
        for f in fields.iter().filter(|f| !f.skip) {
            let ty = &f.ty;
            let member = &f.member;
            seed = quote!(#base::combine(#seed,#base::combine(#base::signature(stringify!(#member)),<#ty as #base::WireWrite>::SIGNATURE)));
        }
        seed
    };
    let mut layout = None;
    let mut variant_fields = vec![];
    let mut signature =
        quote!(#base::signature(concat!(module_path!(),"::",stringify!(#name),"::wire-v1")));
    let (encode, expansion, cost, decode, variants) = match input.data {
        syn::Data::Struct(data) => {
            let fields = selected(&data.fields)?;
            signature = field_signature(signature, &fields);
            layout = Some(field_signature(
                quote!(#base::signature("record-fields-v1")),
                &fields,
            ));
            let mut writes = vec![];
            let mut sizes = vec![];
            let mut costs = vec![];
            let mut values = vec![];
            for f in &fields {
                let ty = &f.ty;
                let member = &f.member;
                if !f.skip {
                    writes.push(quote!(#base::WireWrite::encode(&self.#member,out)?;));
                    sizes.push(
                        quote!(.saturating_add(#base::WireWrite::expansion_bytes(&self.#member))),
                    );
                    costs.push(
                        quote!(.saturating_add(<#ty as #base::WireRead>::input_cost(input)?)),
                    );
                }
                let value = if f.skip {
                    quote!(Default::default())
                } else {
                    quote!(<#ty as #base::WireRead>::decode(input)?)
                };
                values.push((member, value));
            }
            let constructor = match data.fields {
                Fields::Named(_) => {
                    let values = values.iter().map(|(m, v)| quote!(#m:#v));
                    quote!(Self{#(#values),*})
                }
                Fields::Unnamed(_) => {
                    let values = values.iter().map(|(_, v)| v);
                    quote!(Self(#(#values),*))
                }
                Fields::Unit => quote!(Self),
            };
            (
                quote!(#(#writes)* Ok(())),
                quote!(512usize #(#sizes)*),
                quote!(Ok(std::mem::size_of::<Self>() #(#costs)*)),
                quote!(Ok(#constructor)),
                vec![],
            )
        }
        syn::Data::Enum(data) => {
            let mut writes = vec![];
            let mut sizes = vec![];
            let mut costs = vec![];
            let mut values = vec![];
            let mut variants = vec![];
            for (index, variant) in data.variants.into_iter().enumerate() {
                if binary_skip(&variant.attrs)? {
                    return Err(syn::Error::new_spanned(
                        variant.ident,
                        "Skipped binary variants require explicit review",
                    ));
                }
                let ident = &variant.ident;
                let text = ident.to_string();
                variants.push(text.clone());
                let index = index as u32;
                let fields = selected(&variant.fields)?;
                let field_names = fields
                    .iter()
                    .filter(|f| !f.skip)
                    .map(|f| match &f.member {
                        syn::Member::Named(n) => n.to_string(),
                        syn::Member::Unnamed(n) => n.index.to_string(),
                    })
                    .collect::<Vec<_>>();
                variant_fields.push(quote!(&[#(#field_names),*]));
                signature = field_signature(
                    quote!(#base::combine(#signature,#base::signature(#text))),
                    &fields,
                );
                let pattern = match &variant.fields {
                    Fields::Named(_) => {
                        let values = fields.iter().map(|f| {
                            let m = &f.member;
                            let v = &f.variable;
                            quote!(#m:#v)
                        });
                        quote!(Self::#ident{#(#values),*})
                    }
                    Fields::Unnamed(_) => {
                        let values = fields.iter().map(|f| &f.variable);
                        quote!(Self::#ident(#(#values),*))
                    }
                    Fields::Unit => quote!(Self::#ident),
                };
                let mut body = vec![];
                let mut size = quote!(512usize);
                let mut cost = quote!(std::mem::size_of::<Self>());
                let mut constructed = vec![];
                for f in &fields {
                    let variable = &f.variable;
                    let ty = &f.ty;
                    let m = &f.member;
                    if !f.skip {
                        body.push(quote!(#base::WireWrite::encode(#variable,out)?;));
                        size = quote!(#size.saturating_add(#base::WireWrite::expansion_bytes(#variable)));
                        cost = quote!(#cost.saturating_add(<#ty as #base::WireRead>::input_cost(input)?));
                    }
                    let value = if f.skip {
                        quote!(Default::default())
                    } else {
                        quote!(<#ty as #base::WireRead>::decode(input)?)
                    };
                    constructed.push((m, value));
                }
                let constructor = match &variant.fields {
                    Fields::Named(_) => {
                        let fields = constructed.iter().map(|(m, v)| quote!(#m:#v));
                        quote!(Self::#ident{#(#fields),*})
                    }
                    Fields::Unnamed(_) => {
                        let fields = constructed.iter().map(|(_, v)| v);
                        quote!(Self::#ident(#(#fields),*))
                    }
                    Fields::Unit => quote!(Self::#ident),
                };
                writes.push(
                    quote!(#pattern=>{#base::WireWrite::encode(&#index,out)?;#(#body)* Ok(())}),
                );
                sizes.push(quote!(#pattern=>#size));
                costs.push(quote!(#index=>Ok(#cost)));
                values.push(quote!(#index=>Ok(#constructor)));
            }
            (
                quote!(match self {#(#writes),*}),
                quote!(match self {#(#sizes),*}),
                quote!(match <u32 as #base::WireRead>::decode(input)? {#(#costs),*,_=>Err(#base::WireError("Unknown binary variant"))}),
                quote!(match <u32 as #base::WireRead>::decode(input)? {#(#values),*,_=>Err(#base::WireError("Unknown binary variant"))}),
                variants,
            )
        }
        _ => return Err(syn::Error::new_spanned(name, "Binary union is unsupported")),
    };
    let read = read.then(|| {
        quote!(impl #ri #base::WireRead for #name #rt #rw {
            fn input_cost(input:&mut #base::Reader<'_>)->#base::Result<usize>{#cost}
            fn decode(input:&mut #base::Reader<'_>)->#base::Result<Self>{#decode}
        })
    });
    let layout = layout.unwrap_or_else(|| signature.clone());
    Ok(quote!(
        impl #wi #base::WireWrite for #name #wt #ww {
            const SIGNATURE:u128=#signature;
            const FIELD_LAYOUT:u128=#layout;
            const VARIANTS:&'static[&'static str]=&[#(#variants),*];
            const VARIANT_FIELDS:&'static[&'static[&'static str]]=&[#(#variant_fields),*];
            fn encode<W:std::io::Write>(&self,out:&mut W)->#base::Result<()> {#encode}
            fn expansion_bytes(&self)->usize{#expansion}
        }
        #read
    ))
}
