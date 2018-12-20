use std::borrow::Cow;

#[derive(Debug, Default)]
pub struct KVPCollection<'t> {
    kvps: Vec::<KVPStrings<'t>>
}

impl<'t> KVPCollection<'t> {
    /// Insert a new KVP, but only if it does not already exist.
    pub fn insert(&mut self, new_kvp: KVPStrings<'t>) {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(new_kvp.key) {
                return;
            }
        }

        self.kvps.push(new_kvp);
    }

    /// Gets a KVP, looking it up case-insensitively by the specified key.
    pub fn get(&self, key: &str) -> Option<&KVPStrings> {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(key) {
                return Some(&kvp);
            }
        }

        None
    }

    /// Gets a value, looking it up case-insensitively by the specified key.
    pub fn get_value(&self, key: &str) -> Option<&str> {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(key) {
                return Some(&kvp.value);
            }
        }

        None
    }

    pub fn len(&self) -> usize {
        self.kvps.len()
    }

    // pub fn contains(&self, key: &str) -> bool {
    //     match self.get(key) {
    //         Some(_) => true,
    //         None => false
    //     }
    // }
}

/// Represents a KVP as string slices from the original line.
/// The value may be an original slice, or it may be a String if it required
/// cleanup, e.g. if the original value contained embedded newlines.
#[derive(Debug, Default)]
pub struct KVPStrings<'t> {
    key: &'t str,
    value: Cow<'t, str>,
}

/// Represents a KVP as it is parsed out of a line. This is low-level information
/// internal to this module and is used to construct a `KVPStrings` object.
struct KVPParseData {
    key_start_index: usize,
    key_end_index: usize,
    value_start_index: usize,
    value_end_index: usize,
    is_log_level: bool,
    value_is_quoted: bool
}


#[cfg(test)]
mod kvp_collection_tests {
    use super::*;

    #[test]
    pub fn insert_does_not_add_if_strings_equal() {
        let mut sut = KVPCollection::default();
        sut.insert(KVPStrings { key: "car", value: "ford".into() });
        sut.insert(KVPStrings { key: "car", value: "volvo".into() });
        
        assert_eq!(sut.len(), 1);
        assert_eq!(sut.get_value("car").unwrap(), "ford");
    }

    #[test]
    pub fn insert_adds_if_strings_different() {
        let mut sut = KVPCollection::default();
        sut.insert(KVPStrings { key: "car", value: "ford".into() });
        sut.insert(KVPStrings { key: "truck", value: "volvo".into() });
        
        assert_eq!(sut.len(), 2);
        assert_eq!(sut.get_value("car").unwrap(), "ford");
        assert_eq!(sut.get_value("truck").unwrap(), "volvo");
    }

    #[test]
    pub fn get_value_works_case_insensitively() {
        let mut sut = KVPCollection::default();
        sut.insert(KVPStrings { key: "car", value: "ford".into() });

        assert_eq!(sut.len(), 1);
        assert_eq!(sut.get_value("car").unwrap(), "ford");
        assert_eq!(sut.get_value("Car").unwrap(), "ford");
    }
}
