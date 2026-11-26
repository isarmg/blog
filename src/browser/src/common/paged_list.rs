//! Counts and overlap handling shared by the blog's paged lists.
use crate::contracts::MAX_PAGE;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) fn unsigned_field(value: &Value, key: &str) -> u64 {
    value[key].as_u64().expect("validated nonnegative integer")
}

pub(crate) trait PageMetadata {
    fn counts(&self) -> (u64, u64);
}

impl PageMetadata for Value {
    fn counts(&self) -> (u64, u64) {
        (unsigned_field(self, "total"), unsigned_field(self, "size"))
    }
}

impl<T> PageMetadata for crate::models::Page<T> {
    fn counts(&self) -> (u64, u64) {
        (self.total, self.size)
    }
}

pub(crate) fn last_page(total: u64, size: u64) -> u64 {
    total.div_ceil(size).clamp(1, MAX_PAGE)
}

// Offset pages can overlap after a new item is published. Preserve server order.
pub(crate) fn merge_page(current: &mut Vec<Value>, page: u64, next: Vec<Value>) {
    if page == 1 {
        *current = next;
    } else {
        let mut ids: HashSet<_> = current.iter().map(|v| unsigned_field(v, "id")).collect();
        current.extend(
            next.into_iter()
                .filter(|v| ids.insert(unsigned_field(v, "id"))),
        );
    }
}

#[cfg(test)]
#[path = "../../../../test/browser/unit/common/pagination/tests.rs"]
mod tests;
