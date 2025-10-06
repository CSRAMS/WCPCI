use anyhow::Result;

use super::object::{Object, ObjectType};

#[derive(Debug, Clone)]
pub struct Leaf {
    mode: String,
    hash: Vec<u8>,
    name: String,
}

impl Leaf {
    pub fn new(mode: String, hash: Vec<u8>, name: String) -> Self {
        Self { mode, hash, name }
    }
}

pub struct Tree {
    entries: Vec<Leaf>,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, mode: String, hash: Vec<u8>, name: String) {
        self.entries.push(Leaf::new(mode, hash, name));
        self.entries.sort_by(|a, b| a.name.cmp(&b.name));
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut res = Vec::new();
        for entry in &self.entries {
            res.extend_from_slice(entry.mode.as_bytes());
            res.push(b' ');
            res.extend_from_slice(entry.name.as_bytes());
            res.push(b'\0');
            res.extend_from_slice(&entry.hash);
        }
        res
    }

    pub fn to_object(&self) -> Result<Object> {
        let serialized = self.serialize();
        Object::new(serialized, ObjectType::Tree)
    }
}
