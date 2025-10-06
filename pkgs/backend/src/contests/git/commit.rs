use anyhow::Result;

use super::object::{Object, ObjectType};

pub struct Commit {
    tree: String,
    parent: String,
    author: String,
    committer: String,
    gpgsig: String,
    message: String,
}

impl Commit {
    pub fn new(
        tree: String,
        parent: String,
        author: String,
        committer: String,
        gpgsig: String,
        message: String,
    ) -> Self {
        Commit {
            tree,
            parent,
            author,
            committer,
            gpgsig,
            message,
        }
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut serialized = Vec::with_capacity(75);
        serialized.extend_from_slice(b"tree ");
        serialized.extend_from_slice(self.tree.as_bytes());
        serialized.push(b'\n');
        if !self.parent.is_empty() {
            serialized.extend_from_slice(b"parent ");
            serialized.extend_from_slice(self.parent.as_bytes());
            serialized.push(b'\n');
        }
        serialized.extend_from_slice(b"author ");
        serialized.extend_from_slice(self.author.as_bytes());
        serialized.push(b'\n');
        serialized.extend_from_slice(b"committer ");
        serialized.extend_from_slice(self.committer.as_bytes());
        serialized.push(b'\n');
        if !self.gpgsig.is_empty() {
            serialized.extend_from_slice(b"gpgsig ");
            serialized.extend_from_slice(self.gpgsig.as_bytes());
            serialized.push(b'\n');
        }
        serialized.push(b'\n');
        serialized.extend_from_slice(self.message.as_bytes());
        Ok(serialized)
    }

    pub fn to_object(&self) -> Result<Object> {
        let serialized = self.serialize()?;
        Object::new(serialized, ObjectType::Commit)
    }
}
