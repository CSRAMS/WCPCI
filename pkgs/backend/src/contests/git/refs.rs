use std::fmt::{Display, Formatter};

pub enum Ref {
    Object(String),
    Forward(String),
}

impl Ref {
    pub fn get_hash(&self) -> &str {
        match self {
            Ref::Object(hash) => hash,
            Ref::Forward(_) => panic!("Cannot get hash of a forward ref"),
        }
    }
}

impl Display for Ref {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let str = match self {
            Ref::Object(hash) => hash.to_string(),
            Ref::Forward(label) => format!("ref: {}", label),
        };
        write!(f, "{str}")
    }
}
