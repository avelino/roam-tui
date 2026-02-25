pub fn daily_note_uid_for_date(month: u32, day: u32, year: i32) -> String {
    format!("{:02}-{:02}-{}", month, day, year)
}

pub fn pull_daily_note(uid: &str) -> (serde_json::Value, String) {
    let eid = serde_json::Value::String(format!("[:block/uid \"{}\"]", uid));
    let selector = page_selector();
    (eid, selector)
}

pub fn pull_page_by_title(title: &str) -> (serde_json::Value, String) {
    let eid = serde_json::Value::String(format!("[:node/title \"{}\"]", title));
    let selector = page_selector();
    (eid, selector)
}

fn page_selector() -> String {
    "[:node/title :block/uid {:block/children [:block/uid :block/string :block/order :block/open {:block/children ...}]}]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_note_uid_formats_correctly() {
        assert_eq!(daily_note_uid_for_date(2, 21, 2026), "02-21-2026");
        assert_eq!(daily_note_uid_for_date(12, 1, 2025), "12-01-2025");
        assert_eq!(daily_note_uid_for_date(1, 5, 2026), "01-05-2026");
    }

    #[test]
    fn pull_daily_note_returns_correct_eid_format() {
        let (eid, _selector) = pull_daily_note("02-21-2026");
        assert_eq!(
            eid,
            serde_json::Value::String("[:block/uid \"02-21-2026\"]".into())
        );
    }

    #[test]
    fn pull_daily_note_selector_includes_children() {
        let (_eid, selector) = pull_daily_note("02-21-2026");
        assert!(selector.contains(":node/title"));
        assert!(selector.contains(":block/uid"));
        assert!(selector.contains(":block/children"));
        assert!(selector.contains(":block/string"));
        assert!(selector.contains(":block/order"));
    }

    #[test]
    fn pull_daily_note_works_with_generated_uid() {
        let uid = daily_note_uid_for_date(2, 21, 2026);
        let (eid, _) = pull_daily_note(&uid);
        assert_eq!(
            eid,
            serde_json::Value::String("[:block/uid \"02-21-2026\"]".into())
        );
    }

    #[test]
    fn pull_page_by_title_uses_node_title_eid() {
        let (eid, _selector) = pull_page_by_title("My Page");
        assert_eq!(
            eid,
            serde_json::Value::String("[:node/title \"My Page\"]".into())
        );
    }

    #[test]
    fn pull_page_by_title_uses_same_selector_as_daily_note() {
        let (_, daily_selector) = pull_daily_note("02-21-2026");
        let (_, page_selector) = pull_page_by_title("My Page");
        assert_eq!(daily_selector, page_selector);
    }

    #[test]
    fn pull_page_by_title_with_special_chars() {
        let (eid, _) = pull_page_by_title("C++ / Rust");
        assert_eq!(
            eid,
            serde_json::Value::String("[:node/title \"C++ / Rust\"]".into())
        );
    }
}
