mod all;
mod fuzzy_match_room_name;
mod logical_all;
mod logical_any;
mod logical_not;
mod non_left;
mod none;
mod normalized_match_room_name;

pub use all::new_filter as new_filter_all;
pub use fuzzy_match_room_name::new_filter as new_filter_fuzzy_match_room_name;
pub use logical_all::new_filter as new_filter_logical_all;
pub use logical_any::new_filter as new_filter_logical_any;
pub use logical_not::new_filter as new_filter_logical_not;
use matrix_sdk::RoomListEntry;
pub use non_left::new_filter as new_filter_non_left;
pub use none::new_filter as new_filter_none;
pub use normalized_match_room_name::new_filter as new_filter_normalized_match_room_name;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

/// A trait “alias” that represents a _filter_.
///
/// A filter is simply a function that receives a `&RoomListEntry` and returns a
/// `bool`.
pub trait Filter: Fn(&RoomListEntry) -> bool {}

impl<F> Filter for F where F: Fn(&RoomListEntry) -> bool {}

/// Normalize a string, i.e. decompose it into NFD (Normalization Form D, i.e. a
/// canonical decomposition, see http://www.unicode.org/reports/tr15/) and
/// filter out the combining marks.
fn normalize_string(str: &str) -> String {
    str.nfd().filter(|c| !is_combining_mark(*c)).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::normalize_string;

    #[test]
    fn test_normalize_string() {
        assert_eq!(&normalize_string("abc"), "abc");
        assert_eq!(&normalize_string("Ștefan Été"), "Stefan Ete");
        assert_eq!(&normalize_string("Ç ṩ ḋ Å"), "C s d A");
        assert_eq!(&normalize_string("هند"), "هند");
    }
}
