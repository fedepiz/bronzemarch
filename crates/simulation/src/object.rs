use std::collections::BTreeMap;

use crate::{EntityId, SiteId};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct ObjectId(pub(crate) ObjectHandle);

impl ObjectId {
    pub fn global() -> Self {
        Self(ObjectHandle::Global)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ObjectHandle {
    Null,
    Global,
    Site(SiteId),
    Entity(EntityId),
}

impl Default for ObjectHandle {
    fn default() -> Self {
        Self::Null
    }
}

#[derive(Default)]
pub struct Object(BTreeMap<String, Value>);

pub(crate) enum Value {
    Id(ObjectId),
    Flag(bool),
    String(String),
    Child(Object),
    List(Vec<Object>),
}

impl From<ObjectId> for Value {
    fn from(value: ObjectId) -> Self {
        Value::Id(value)
    }
}

impl From<ObjectHandle> for Value {
    fn from(value: ObjectHandle) -> Self {
        Value::Id(ObjectId(value))
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Flag(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl<'a> From<&'a String> for Value {
    fn from(value: &'a String) -> Self {
        Self::String(value.clone())
    }
}

impl<'a> From<&'a str> for Value {
    fn from(value: &'a str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<Object> for Value {
    fn from(value: Object) -> Self {
        Self::Child(value)
    }
}

impl From<Vec<Object>> for Value {
    fn from(value: Vec<Object>) -> Self {
        Self::List(value)
    }
}

impl Object {
    const EMPTY: &'static Object = &Object::new();

    pub(crate) const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub(crate) fn set(&mut self, tag: impl Into<String>, value: impl Into<Value>) {
        self.0.insert(tag.into(), value.into());
    }

    pub fn id(&self, tag: &str) -> ObjectId {
        match self.0.get(tag) {
            Some(Value::Id(id)) => *id,
            _ => ObjectId(ObjectHandle::Null),
        }
    }

    pub fn txt<'a>(&'a self, tag: &str) -> &'a str {
        self.try_text(tag).unwrap_or("INVALID")
    }

    pub fn try_text<'a>(&'a self, tag: &str) -> Option<&'a str> {
        match self.0.get(tag) {
            Some(Value::String(str)) => Some(str.as_str()),
            _ => None,
        }
    }

    pub fn flag(&self, tag: &str) -> bool {
        match self.0.get(tag) {
            Some(Value::Flag(flag)) => *flag,
            _ => false,
        }
    }

    pub fn child<'a>(&'a self, tag: &str) -> &'a Object {
        self.try_child(tag).unwrap_or(Self::EMPTY)
    }

    pub fn try_child<'a>(&'a self, tag: &str) -> Option<&'a Object> {
        match self.0.get(tag) {
            Some(Value::Child(obj)) => Some(obj),
            _ => None,
        }
    }

    pub fn list<'a>(&'a self, tag: &str) -> &'a [Object] {
        match self.0.get(tag) {
            Some(Value::List(items)) => items.as_slice(),
            _ => &[],
        }
    }
}
