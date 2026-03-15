use crate::api::types::{Block, DailyNote};

pub fn blocks_to_markdown(title: &str, blocks: &[Block]) -> String {
    let mut output = format!("# {}\n\n", title);
    for block in blocks {
        render_block_md(block, 0, &mut output);
    }
    output
}

pub fn daily_notes_to_markdown(days: &[DailyNote]) -> String {
    let mut output = String::new();
    for (i, day) in days.iter().enumerate() {
        if i > 0 {
            output.push_str("\n---\n\n");
        }
        output.push_str(&format!("# {}\n\n", day.title));
        for block in &day.blocks {
            render_block_md(block, 0, &mut output);
        }
    }
    output
}

fn render_block_md(block: &Block, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    if !block.string.is_empty() {
        output.push_str(&format!("{}- {}\n", indent, block.string));
    } else {
        output.push_str(&format!("{}-\n", indent));
    }
    for child in &block.children {
        render_block_md(child, depth + 1, output);
    }
}

pub fn daily_notes_to_json(days: &[DailyNote]) -> String {
    let json: Vec<serde_json::Value> = days
        .iter()
        .map(|day| {
            serde_json::json!({
                "title": day.title,
                "uid": day.uid,
                "date": day.date.to_string(),
                "blocks": blocks_to_json_value(&day.blocks),
            })
        })
        .collect();
    serde_json::to_string_pretty(&json).unwrap_or_default()
}

pub fn blocks_to_json(title: &str, blocks: &[Block]) -> String {
    let json = serde_json::json!({
        "title": title,
        "blocks": blocks_to_json_value(blocks),
    });
    serde_json::to_string_pretty(&json).unwrap_or_default()
}

fn blocks_to_json_value(blocks: &[Block]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .map(|b| {
            serde_json::json!({
                "uid": b.uid,
                "string": b.string,
                "order": b.order,
                "open": b.open,
                "children": blocks_to_json_value(&b.children),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sample_blocks() -> Vec<Block> {
        vec![
            Block {
                uid: "b1".into(),
                string: "First block".into(),
                order: 0,
                children: vec![Block {
                    uid: "c1".into(),
                    string: "Child block".into(),
                    order: 0,
                    children: vec![],
                    open: true,
                    refs: vec![],
                }],
                open: true,
                refs: vec![],
            },
            Block {
                uid: "b2".into(),
                string: "Second block".into(),
                order: 1,
                children: vec![],
                open: true,
                refs: vec![],
            },
        ]
    }

    fn sample_daily_note() -> DailyNote {
        DailyNote {
            date: NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
            uid: "03-15-2026".into(),
            title: "March 15th, 2026".into(),
            blocks: sample_blocks(),
        }
    }

    #[test]
    fn markdown_has_title_heading() {
        let md = blocks_to_markdown("Test Page", &sample_blocks());
        assert!(md.starts_with("# Test Page\n"));
    }

    #[test]
    fn markdown_preserves_hierarchy() {
        let md = blocks_to_markdown("Page", &sample_blocks());
        assert!(md.contains("- First block\n"));
        assert!(md.contains("  - Child block\n"));
        assert!(md.contains("- Second block\n"));
    }

    #[test]
    fn markdown_preserves_roam_syntax() {
        let blocks = vec![Block {
            uid: "b1".into(),
            string: "Link to [[Page]] and ((ref-uid))".into(),
            order: 0,
            children: vec![],
            open: true,
            refs: vec![],
        }];
        let md = blocks_to_markdown("Test", &blocks);
        assert!(md.contains("[[Page]]"));
        assert!(md.contains("((ref-uid))"));
    }

    #[test]
    fn markdown_handles_empty_blocks() {
        let blocks = vec![Block {
            uid: "b1".into(),
            string: String::new(),
            order: 0,
            children: vec![],
            open: true,
            refs: vec![],
        }];
        let md = blocks_to_markdown("Test", &blocks);
        assert!(md.contains("-\n"));
    }

    #[test]
    fn markdown_daily_notes_with_separator() {
        let days = vec![sample_daily_note(), sample_daily_note()];
        let md = daily_notes_to_markdown(&days);
        assert!(md.contains("---"));
        assert_eq!(md.matches("# March 15th, 2026").count(), 2);
    }

    #[test]
    fn json_has_required_fields() {
        let json_str = blocks_to_json("Test Page", &sample_blocks());
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["title"], "Test Page");
        assert!(parsed["blocks"].is_array());
        assert_eq!(parsed["blocks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn json_preserves_block_structure() {
        let json_str = blocks_to_json("Test", &sample_blocks());
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let first = &parsed["blocks"][0];
        assert_eq!(first["uid"], "b1");
        assert_eq!(first["string"], "First block");
        assert_eq!(first["children"][0]["uid"], "c1");
    }

    #[test]
    fn json_daily_notes_has_date() {
        let days = vec![sample_daily_note()];
        let json_str = daily_notes_to_json(&days);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed[0]["date"], "2026-03-15");
        assert_eq!(parsed[0]["uid"], "03-15-2026");
    }

    #[test]
    fn markdown_code_block_preserved() {
        let blocks = vec![Block {
            uid: "b1".into(),
            string: "```rust\nfn main() {}\n```".into(),
            order: 0,
            children: vec![],
            open: true,
            refs: vec![],
        }];
        let md = blocks_to_markdown("Test", &blocks);
        assert!(md.contains("```rust"));
        assert!(md.contains("fn main()"));
    }

    #[test]
    fn json_empty_blocks() {
        let json_str = blocks_to_json("Empty", &[]);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed["blocks"].as_array().unwrap().is_empty());
    }

    #[test]
    fn markdown_deeply_nested() {
        let blocks = vec![Block {
            uid: "l0".into(),
            string: "Level 0".into(),
            order: 0,
            open: true,
            refs: vec![],
            children: vec![Block {
                uid: "l1".into(),
                string: "Level 1".into(),
                order: 0,
                open: true,
                refs: vec![],
                children: vec![Block {
                    uid: "l2".into(),
                    string: "Level 2".into(),
                    order: 0,
                    children: vec![],
                    open: true,
                    refs: vec![],
                }],
            }],
        }];
        let md = blocks_to_markdown("Test", &blocks);
        assert!(md.contains("- Level 0\n"));
        assert!(md.contains("  - Level 1\n"));
        assert!(md.contains("    - Level 2\n"));
    }
}
