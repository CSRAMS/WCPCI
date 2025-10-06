use std::collections::HashMap;

use anyhow::Result;

use super::object::Object;

#[derive(Hash, Debug, Eq, PartialEq)]
struct ObjectAddress {
    folder: String,
    rest: String,
}

impl ObjectAddress {
    pub fn new(hash: &str) -> Result<Self> {
        let first_two = &hash.chars().take(2).collect::<String>();
        let rest = &hash.chars().skip(2).collect::<String>();
        Ok(ObjectAddress {
            folder: first_two.to_string(),
            rest: rest.to_string(),
        })
    }
}

pub struct ObjectStore {
    map: HashMap<ObjectAddress, Object>,
}

impl ObjectStore {
    pub fn new() -> Self {
        ObjectStore {
            map: HashMap::new(),
        }
    }

    pub fn add_object(&mut self, obj: Object) -> Result<()> {
        let hash = obj.get_hash_str();
        let address = ObjectAddress::new(&hash)?;
        self.map.insert(address, obj);
        Ok(())
    }

    pub fn get_by_address(&self, folder: &str, rest: &str) -> Option<&Object> {
        let address = ObjectAddress {
            folder: folder.to_string(),
            rest: rest.to_string(),
        };
        self.map.get(&address)
    }
}
