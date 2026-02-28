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

pub fn linked_refs_query(page_title: &str) -> String {
    let escaped = page_title.replace('"', r#"\""#);
    format!(
        r#"[:find ?uid ?s ?page-title :where [?target :node/title "{}"] [?b :block/refs ?target] [?b :block/uid ?uid] [?b :block/string ?s] [?b :block/page ?p] [?p :node/title ?page-title]]"#,
        escaped
    )
}

pub fn all_page_titles_query() -> String {
    "[:find ?title ?uid :where [?e :node/title ?title] [?e :block/uid ?uid]]".to_string()
}

fn page_selector() -> String {
    "[:block/uid :node/title :block/string {:block/children [:block/uid :block/string :block/order :block/open {:block/refs [:block/uid :node/title :block/string]} {:block/children ...}]}]".to_string()
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
    fn pull_selector_includes_block_refs() {
        let (_eid, selector) = pull_daily_note("02-21-2026");
        assert!(selector.contains(":block/refs"));
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

    #[test]
    fn linked_refs_query_contains_page_title() {
        let q = linked_refs_query("My Page");
        assert!(q.contains("\"My Page\""));
        assert!(q.contains("?uid"));
        assert!(q.contains("?s"));
        assert!(q.contains("?page-title"));
        assert!(q.contains(":block/refs"));
        assert!(q.contains(":node/title"));
        assert!(q.contains(":block/string"));
        assert!(q.contains(":block/uid"));
        assert!(q.contains(":block/page"));
    }

    #[test]
    fn all_page_titles_query_contains_expected_clauses() {
        let q = all_page_titles_query();
        assert!(q.contains("?title"));
        assert!(q.contains("?uid"));
        assert!(q.contains(":node/title"));
        assert!(q.contains(":block/uid"));
        assert!(q.contains(":find"));
        assert!(q.contains(":where"));
    }

    #[test]
    fn linked_refs_query_escapes_double_quotes() {
        let q = linked_refs_query(r#"Page "with" quotes"#);
        assert!(q.contains(r#"Page \"with\" quotes"#));
    }
}
