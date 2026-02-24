pub fn daily_note_uid_for_date(month: u32, day: u32, year: i32) -> String {
    format!("{:02}-{:02}-{}", month, day, year)
}

pub fn pull_daily_note(uid: &str) -> (serde_json::Value, String) {
    let eid = serde_json::Value::String(format!("[:block/uid \"{}\"]", uid));
    let selector = "[:node/title :block/uid {:block/children [:block/uid :block/string :block/order :block/open {:block/children ...}]}]".to_string();
    (eid, selector)
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
}
