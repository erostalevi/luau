//! System font families (for Settings → Fonts → Pick fonts).

use std::collections::BTreeSet;

pub fn list() -> Vec<String> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut set = BTreeSet::new();
    for face in db.faces() {
        if let Some((name, _)) = face.families.first() {
            if !name.starts_with('.') {
                set.insert(name.clone());
            }
        }
    }
    set.into_iter().collect()
}
