//! Match the frozen Python reference's Unicode 16 general-category filtering.
use unicode_general_category::{get_general_category, GeneralCategory};
pub fn is_other(character: char) -> bool {
    matches!(
        get_general_category(character),
        GeneralCategory::Control
            | GeneralCategory::Format
            | GeneralCategory::Surrogate
            | GeneralCategory::PrivateUse
            | GeneralCategory::Unassigned
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_reference_database_and_all_other_categories() {
        assert_eq!(unicode_general_category::UNICODE_VERSION, (16, 0, 0));
        for character in ['\0', '\u{200b}', '\u{e000}', '\u{0378}'] {
            assert!(is_other(character));
        }
        for character in ['A', 'λ', '\u{301}', '\u{1fae9}'] {
            assert!(!is_other(character));
        }
    }
}
