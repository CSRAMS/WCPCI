use std::collections::HashMap;

use super::{object::Object, refs::Ref, store::ObjectStore};

pub struct FakeRepo {
    object_store: ObjectStore,
    pub tags: HashMap<String, Ref>,
    pub heads: HashMap<String, Ref>,
}

impl FakeRepo {
    pub fn new() -> Self {
        FakeRepo {
            object_store: ObjectStore::new(),
            tags: HashMap::new(),
            heads: HashMap::new(),
        }
    }

    pub fn add_tag(&mut self, name: &str, ref_: Ref) {
        self.tags.insert(name.to_string(), ref_);
    }

    pub fn add_head(&mut self, name: &str, ref_: Ref) {
        self.heads.insert(name.to_string(), ref_);
    }

    pub fn add_object(&mut self, obj: Object) {
        self.object_store.add_object(obj).unwrap();
    }

    pub fn get_object(&self, folder: &str, rest: &str) -> Option<&Object> {
        self.object_store.get_by_address(folder, rest)
    }

    pub fn dump_refs(&self) -> String {
        let mut refs = String::new();
        for (name, ref_) in self.heads.iter() {
            refs.push_str(&format!("{}\trefs/heads/{}\n", ref_.get_hash(), name));
        }
        for (name, ref_) in self.tags.iter() {
            refs.push_str(&format!("{}\trefs/tags/{}\n", ref_.get_hash(), name));
        }
        refs
    }
}
