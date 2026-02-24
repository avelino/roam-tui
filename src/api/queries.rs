#[allow(dead_code)]
pub fn all_page_titles() -> String {
    "[:find ?title ?uid :where [?e :node/title ?title] [?e :block/uid ?uid]]".into()
}

#[allow(dead_code)]
pub fn page_by_title(title: &str) -> (String, Vec<serde_json::Value>) {
    let query =
        "[:find ?uid :in $ ?title :where [?e :node/title ?title] [?e :block/uid ?uid]]".into();
    let args = vec![serde_json::Value::String(title.to_string())];
    (query, args)
}

pub fn daily_note_uid_for_date(month: u32, day: u32, year: i32) -> String {
    format!("{:02}-{:02}-{}", month, day, year)
}

#[allow(dead_code)]
pub fn pull_pattern_full_page() -> String {
    "[:block/uid :block/string :block/order :block/open {:block/children ...}]".into()
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
    fn all_page_titles_returns_valid_datalog() {
        let q = all_page_titles();
        assert!(q.starts_with("[:find"));
        assert!(q.contains(":node/title"));
        assert!(q.contains(":block/uid"));
        assert!(q.contains("?title"));
        assert!(q.contains("?uid"));
    }

    #[test]
    fn page_by_title_returns_query_and_args() {
        let (q, args) = page_by_title("My Page");
        assert!(q.contains(":in $ ?title"));
        assert!(q.contains(":node/title ?title"));
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "My Page");
    }

    #[test]
    fn daily_note_uid_formats_correctly() {
        assert_eq!(daily_note_uid_for_date(2, 21, 2026), "02-21-2026");
        assert_eq!(daily_note_uid_for_date(12, 1, 2025), "12-01-2025");
        assert_eq!(daily_note_uid_for_date(1, 5, 2026), "01-05-2026");
    }

    #[test]
    fn pull_pattern_contains_expected_attributes() {
        let pattern = pull_pattern_full_page();
        assert!(pattern.contains(":block/uid"));
        assert!(pattern.contains(":block/string"));
        assert!(pattern.contains(":block/order"));
        assert!(pattern.contains(":block/children"));
    }

    #[test]
    fn pull_daily_note_returns_correct_eid_format() {
        let (eid, _selector) = pull_daily_note("02-21-2026");
        assert_eq!(eid, serde_json::Value::String("[:block/uid \"02-21-2026\"]".into()));
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
        assert_eq!(eid, serde_json::Value::String("[:block/uid \"02-21-2026\"]".into()));
    }
}
