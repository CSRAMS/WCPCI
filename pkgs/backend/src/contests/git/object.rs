use std::io::Write;

use anyhow::{Context, Result};
use flate2::{write::ZlibEncoder, Compression};
use sha1::{Digest, Sha1};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ObjectType {
    Blob,
    Tree,
    Commit,
}

impl ObjectType {
    fn to_header(self) -> &'static str {
        match self {
            ObjectType::Blob => "blob",
            ObjectType::Tree => "tree",
            ObjectType::Commit => "commit",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Object {
    contents: Vec<u8>,
    o_type: ObjectType,
}

impl Object {
    pub fn new(contents: Vec<u8>, o_type: ObjectType) -> Result<Self> {
        Ok(Object { contents, o_type })
    }

    pub fn get_hash(&self) -> Vec<u8> {
        let mut hasher = Sha1::new();
        hasher.update(self.serialize());
        hasher.finalize().to_vec()
    }

    pub fn get_hash_str(&self) -> String {
        let mut hasher = Sha1::new();
        hasher.update(self.serialize());
        format!("{:x}", hasher.finalize())
    }

    pub fn serialize(&self) -> Vec<u8> {
        // format: <type> <size>\0<contents>
        let mut res = Vec::new();
        res.extend_from_slice(self.o_type.to_header().as_bytes());
        res.push(b' ');
        res.extend_from_slice(self.contents.len().to_string().as_bytes());
        res.push(b'\0');
        res.extend_from_slice(&self.contents);
        res
    }

    pub fn compressed_serialize(&self) -> Result<Vec<u8>> {
        let dat = self.serialize();
        let mut encoder = ZlibEncoder::new(Vec::with_capacity(dat.len()), Compression::default());
        encoder.write_all(&dat).context("Failed to compress")?;
        encoder.finish().context("Failed to finish compression")
    }
}

impl Default for Object {
    fn default() -> Self {
        Object {
            contents: Vec::new(),
            o_type: ObjectType::Blob,
        }
    }
}
