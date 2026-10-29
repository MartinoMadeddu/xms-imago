//! Metadata: `key=value` pairs separated by `;`, on the archive, each object
//! and each property (`schema=AbcGeom_PolyMesh_v1`, `geoScope=vtx`,
//! `interpretation=point`).

use std::collections::BTreeMap;

/// The metadata of an archive, an object or a property, by key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetaData(pub BTreeMap<String, String>);

impl MetaData {
    /// Read from its text: the first value of a key is kept, as Alembic does.
    pub fn parse(text: &str) -> MetaData {
        let mut map = BTreeMap::new();
        for pair in text.split(';') {
            if let Some((k, v)) = pair.split_once('=') {
                map.entry(k.to_string()).or_insert_with(|| v.to_string());
            }
        }
        MetaData(map)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_reads_its_pairs() {
        let m = MetaData::parse("schema=AbcGeom_PolyMesh_v1;schemaObjTitle=AbcGeom_PolyMesh_v1:.geom;schema=x");
        assert_eq!(m.get("schema"), Some("AbcGeom_PolyMesh_v1"), "the first wins");
        assert_eq!(m.get("schemaObjTitle"), Some("AbcGeom_PolyMesh_v1:.geom"));
        assert!(MetaData::parse("").is_empty());
    }
}
