use super::{invalid, Result};
use std::{collections::HashMap, str::FromStr};
pub type Node<'a> = roxmltree::Node<'a, 'a>;
pub struct Graph<'a> {
    ids: HashMap<&'a str, Node<'a>>,
}
impl<'a> Graph<'a> {
    pub fn new(doc: &'a roxmltree::Document<'a>) -> Result<Self> {
        let mut ids = HashMap::new();
        for n in doc.descendants().filter(|n| n.is_element()) {
            if let Some(id) = n.attribute("xs.id") {
                if ids.insert(id, n).is_some() {
                    return Err(invalid("Duplicate XML object identity"));
                }
            }
        }
        let graph = Self { ids };
        for n in doc.descendants().filter(|n| n.is_element()) {
            graph.resolve(n)?;
        }
        Ok(graph)
    }
    pub fn resolve(&self, mut n: Node<'a>) -> Result<Node<'a>> {
        for _ in 0..32 {
            let Some(id) = n.attribute("xs.ref") else {
                return Ok(n);
            };
            n = *self
                .ids
                .get(id)
                .ok_or_else(|| invalid(format!("Missing XML reference {id}")))?;
        }
        Err(invalid("Cyclic XML reference"))
    }
    pub fn field(&self, n: Node<'a>, name: &str) -> Result<Option<Node<'a>>> {
        let mut n = self.resolve(n)?;
        for _ in 0..16 {
            if let Some(c) = n.children().find(|c| c.attribute("xs.n") == Some(name)) {
                return self.resolve(c).map(Some);
            }
            let Some(base) = n.children().find(|c| c.attribute("xs.n") == Some("super")) else {
                return Ok(None);
            };
            n = self.resolve(base)?;
        }
        Err(invalid("Excessive XML inheritance depth"))
    }
    pub fn req(&self, n: Node<'a>, name: &str) -> Result<Node<'a>> {
        self.field(n, name)?
            .ok_or_else(|| invalid(format!("Missing {}.{name}", n.tag_name().name())))
    }
    pub fn list(&self, n: Node<'a>, name: &str) -> Result<Vec<Node<'a>>> {
        let Some(list) = self.field(n, name)? else {
            return Ok(vec![]);
        };
        list.children()
            .filter(|n| n.is_element())
            .map(|n| self.resolve(n))
            .collect()
    }
    pub fn text(&self, n: Node<'a>, name: &str) -> Result<String> {
        Ok(self.req(n, name)?.text().unwrap_or("").into())
    }
    pub fn number<T: FromStr>(&self, n: Node<'a>, name: &str) -> Result<T> {
        parse(self.req(n, name)?.text().unwrap_or(""))
    }
    pub fn optional<T: FromStr>(&self, n: Node<'a>, name: &str, default: T) -> Result<T> {
        self.field(n, name)?
            .map_or(Ok(default), |n| parse(n.text().unwrap_or("")))
    }
    pub fn guid(&self, n: Node<'a>, name: &str) -> Result<String> {
        Ok(self.req(n, name)?.attribute("uuid").unwrap_or("").into())
    }
    pub fn runtime(&self, n: Node<'a>) -> Result<String> {
        Ok(self.req(n, "id")?.attribute("idstr").unwrap_or("").into())
    }
    pub fn floats(&self, n: Node<'a>, name: &str) -> Result<Vec<f32>> {
        let v: Vec<f32> = self.array(n, name)?;
        if v.iter().any(|n| !n.is_finite()) {
            return Err(invalid("Non-finite array value"));
        }
        Ok(v)
    }
    pub fn array<T: FromStr>(&self, n: Node<'a>, name: &str) -> Result<Vec<T>> {
        self.req(n, name)?
            .text()
            .unwrap_or("")
            .split_whitespace()
            .map(parse)
            .collect()
    }
}
pub fn parse<T: FromStr>(v: &str) -> Result<T> {
    v.trim()
        .parse()
        .map_err(|_| invalid(format!("Invalid value: {v}")))
}
pub fn attr<T: FromStr>(n: Node<'_>, name: &str) -> Result<T> {
    parse(
        n.attribute(name)
            .ok_or_else(|| invalid(format!("Missing attribute {name}")))?,
    )
}
