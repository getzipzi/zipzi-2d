//! Small object-graph writer for the Cubism XML serialization protocol.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub tag: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Node>,
    pub text: String,
}
impl Node {
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.into(),
            attrs: BTreeMap::new(),
            children: Vec::new(),
            text: String::new(),
        }
    }
    pub fn attr(mut self, key: &str, value: impl ToString) -> Self {
        self.attrs.insert(key.into(), value.to_string());
        self
    }
    pub fn named(self, name: &str) -> Self {
        self.attr("xs.n", name)
    }
    pub fn child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }
    pub fn field(self, name: &str, value: Self) -> Self {
        self.child(value.named(name))
    }
    pub fn scalar(tag: &str, value: impl ToString) -> Self {
        let mut n = Self::new(tag);
        n.text = value.to_string();
        n
    }
    pub fn list(tag: &str, values: impl IntoIterator<Item = Self>) -> Self {
        let children: Vec<_> = values.into_iter().collect();
        let mut n = Self::new(tag).attr("count", children.len());
        n.children = children;
        n
    }
    pub fn array(tag: &str, values: impl IntoIterator<Item = impl ToString>) -> Self {
        let values: Vec<_> = values.into_iter().map(|v| v.to_string()).collect();
        Self::scalar(tag, values.join(" ")).attr("count", values.len())
    }
    pub fn enumeration(tag: &str, value: &str) -> Self {
        Self::new(tag).attr("v", value)
    }
    pub fn null() -> Self {
        Self::new("null")
    }
    pub fn empty_list() -> Self {
        Self::list("carray_list", [])
    }
    pub fn empty_map() -> Self {
        Self::new("hash_map")
            .attr("count", 0)
            .attr("keyType", "string")
    }
    pub fn field_value(&self, name: &str) -> Option<&Self> {
        self.children
            .iter()
            .find(|c| c.attrs.get("xs.n").is_some_and(|v| v == name))
            .or_else(|| {
                self.children
                    .iter()
                    .find(|c| c.attrs.get("xs.n").is_some_and(|v| v == "super"))
                    .and_then(|c| c.field_value(name))
            })
    }
    pub fn field_mut(&mut self, name: &str) -> Option<&mut Self> {
        if let Some(index) = self
            .children
            .iter()
            .position(|c| c.attrs.get("xs.n").is_some_and(|v| v == name))
        {
            return self.children.get_mut(index);
        }
        self.children
            .iter_mut()
            .find(|c| c.attrs.get("xs.n").is_some_and(|v| v == "super"))
            .and_then(|c| c.field_mut(name))
    }
    fn validate(&self) -> Result<(), crate::Cmo3Error> {
        let valid = |s: &str| {
            s.chars().all(|c| {
                matches!(c, '\t' | '\n' | '\r')
                    || (c >= '\u{20}' && c != '\u{fffe}' && c != '\u{ffff}')
            })
        };
        if !valid(&self.text) || self.attrs.values().any(|v| !valid(v)) {
            return Err(crate::Cmo3Error::InvalidDocument(
                "XML text contains an invalid control character".into(),
            ));
        }
        let numeric = if self.tag == "f" || self.tag == "float-array" {
            self.text
                .split_whitespace()
                .any(|v| v.parse::<f32>().map_or(true, |f| !f.is_finite()))
        } else {
            false
        };
        if numeric
            || [
                "originX", "originY", "angle", "scale", "red", "green", "blue", "alpha", "m00",
                "m01", "m02", "m10", "m11", "m12",
            ]
            .iter()
            .any(|attr| {
                self.attrs
                    .get(*attr)
                    .is_some_and(|v| v.parse::<f32>().map_or(true, |f| !f.is_finite()))
            })
        {
            return Err(crate::Cmo3Error::InvalidDocument(
                "Exported form overflows float32".into(),
            ));
        }
        for child in &self.children {
            child.validate()?;
        }
        Ok(())
    }
    fn tags(&self, tags: &mut BTreeSet<String>) {
        tags.insert(self.tag.clone());
        if self.tag == "array" {
            if let Some(item_type) = self.attrs.get("type") {
                tags.insert(item_type.clone());
            }
        }
        for child in &self.children {
            child.tags(tags);
        }
    }
    fn write(&self, out: &mut String) {
        write!(out, "<{}", self.tag).unwrap();
        for (k, v) in &self.attrs {
            write!(out, " {k}=\"{}\"", escape(v)).unwrap();
        }
        if self.children.is_empty() && self.text.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        out.push_str(&escape(&self.text));
        for child in &self.children {
            child.write(out);
        }
        write!(out, "</{}>", self.tag).unwrap();
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}

#[derive(Default)]
pub(crate) struct Graph {
    nodes: Vec<Node>,
}
impl Graph {
    /// Reserve before filling, so source/form and texture/filter cycles are explicit.
    pub fn reserve(&mut self, tag: &str) -> Node {
        let id = self.nodes.len();
        self.nodes.push(Node::new(tag));
        Node::new(tag).attr("xs.ref", id)
    }
    pub fn set(&mut self, reference: &Node, value: Node) {
        let id: usize = reference.attrs["xs.ref"].parse().unwrap();
        self.nodes[id] = value.attr("xs.id", id).attr("xs.idx", id);
    }
    pub fn add(&mut self, value: Node) -> Node {
        let r = self.reserve(&value.tag);
        self.set(&r, value);
        r
    }
    pub fn get(&self, reference: &Node) -> &Node {
        &self.nodes[reference.attrs["xs.ref"].parse::<usize>().unwrap()]
    }
    pub fn document(&self, model: Node) -> Result<Vec<u8>, crate::Cmo3Error> {
        self.document_using(
            model,
            crate::schema::class_name,
            crate::schema::version,
            &[],
        )
    }
    pub fn document_using(
        &self,
        model: Node,
        class_name: fn(&str) -> Option<&'static str>,
        version: fn(&str) -> Option<(&'static str, u32)>,
        extra_versions: &[(&str, u32)],
    ) -> Result<Vec<u8>, crate::Cmo3Error> {
        let root = Node::new("root")
            .attr("fileFormatVersion", "503000000")
            .child(Node::new("shared").child_list(self.nodes.clone()))
            .child(Node::new("main").child(model));
        root.validate()?;
        let mut tags = BTreeSet::new();
        root.tags(&mut tags);
        let mut imports = BTreeSet::new();
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        for (class, version) in extra_versions {
            writeln!(out, "<?version {class}:{version}?>").unwrap();
        }
        for tag in tags {
            if let Some((class, version)) = version(&tag) {
                writeln!(out, "<?version {class}:{version}?>").unwrap();
            }
            if let Some(fqcn) = class_name(&tag) {
                imports.insert(fqcn);
            } else if !crate::schema::is_builtin(&tag) {
                return Err(crate::Cmo3Error::InvalidDocument(format!(
                    "Unknown Cubism tag: {tag}"
                )));
            }
        }
        for fqcn in imports {
            writeln!(out, "<?import {fqcn}?>").unwrap();
        }
        root.write(&mut out);
        Ok(out.into_bytes())
    }
}
impl Node {
    fn child_list(mut self, children: Vec<Node>) -> Self {
        self.children = children;
        self
    }
}
