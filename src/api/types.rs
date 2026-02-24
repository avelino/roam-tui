use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct PullRequest {
    #[serde(rename = "eid")]
    pub eid: serde_json::Value,
    pub selector: String,
}

#[derive(Debug, Deserialize)]
pub struct PullResponse {
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub uid: String,
    pub string: String,
    pub order: i64,
    #[serde(default)]
    pub children: Vec<Block>,
    #[serde(default)]
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DailyNote {
    pub date: NaiveDate,
    pub uid: String,
    pub title: String,
    pub blocks: Vec<Block>,
}

impl DailyNote {
    pub fn from_pull_response(date: NaiveDate, uid: String, result: &serde_json::Value) -> Self {
        let title = result
            .get(":node/title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let blocks = result
            .get(":block/children")
            .and_then(|v| v.as_array())
            .map(|arr| {
                let mut blocks: Vec<Block> = arr.iter().map(parse_block_from_json).collect();
                blocks.sort_by_key(|b| b.order);
                blocks
            })
            .unwrap_or_default();

        Self {
            date,
            uid,
            title,
            blocks,
        }
    }
}

fn parse_block_from_json(val: &serde_json::Value) -> Block {
    let uid = val
        .get(":block/uid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let string = val
        .get(":block/string")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let order = val
        .get(":block/order")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let open = val
        .get(":block/open")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let mut children: Vec<Block> = val
        .get(":block/children")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(parse_block_from_json).collect())
        .unwrap_or_default();
    children.sort_by_key(|b| b.order);

    Block {
        uid,
        string,
        order,
        children,
        open,
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "action")]
#[allow(clippy::enum_variant_names)]
pub enum WriteAction {
    #[serde(rename = "create-block")]
    CreateBlock {
        location: BlockLocation,
        block: NewBlock,
    },
    #[serde(rename = "update-block")]
    UpdateBlock { block: BlockUpdate },
    #[serde(rename = "delete-block")]
    DeleteBlock { block: BlockRef },
    #[serde(rename = "move-block")]
    MoveBlock {
        block: BlockRef,
        location: BlockLocation,
    },
}

#[derive(Debug, Serialize)]
pub struct BlockLocation {
    #[serde(rename = "parent-uid")]
    pub parent_uid: String,
    pub order: OrderValue,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum OrderValue {
    Index(i64),
    Position(String),
}

#[derive(Debug, Serialize)]
pub struct NewBlock {
    pub string: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct BlockUpdate {
    pub uid: String,
    pub string: String,
}

#[derive(Debug, Serialize)]
pub struct BlockRef {
    pub uid: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pull_request_serializes() {
        let req = PullRequest {
            eid: json!(["block/uid", "abc123"]),
            selector: "[:block/string :block/uid {:block/children ...}]".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["eid"], json!(["block/uid", "abc123"]));
        assert!(json["selector"].is_string());
    }

    #[test]
    fn pull_response_deserializes() {
        let raw =
            r#"{"result": {":block/uid": "abc123", ":block/string": "hello", ":block/order": 0}}"#;
        let resp: PullResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.result[":block/uid"], "abc123");
    }

    #[test]
    fn block_serde_roundtrip() {
        let block = Block {
            uid: "def456".into(),
            string: "Hello [[world]]".into(),
            order: 0,
            children: vec![Block {
                uid: "ghi789".into(),
                string: "Child block".into(),
                order: 0,
                children: vec![],
                open: true,
            }],
            open: true,
        };
        let json = serde_json::to_string(&block).unwrap();
        let deserialized: Block = serde_json::from_str(&json).unwrap();
        assert_eq!(block, deserialized);
        assert_eq!(deserialized.children.len(), 1);
    }

    #[test]
    fn block_deserializes_without_optional_fields() {
        let raw = r#"{"uid": "abc", "string": "test", "order": 0}"#;
        let block: Block = serde_json::from_str(raw).unwrap();
        assert!(block.children.is_empty());
        assert!(!block.open);
    }

    #[test]
    fn write_action_create_block_serializes() {
        let action = WriteAction::CreateBlock {
            location: BlockLocation {
                parent_uid: "page-uid".into(),
                order: OrderValue::Position("last".into()),
            },
            block: NewBlock {
                string: "New block content".into(),
                uid: None,
                open: None,
            },
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["action"], "create-block");
        assert_eq!(json["location"]["parent-uid"], "page-uid");
        assert_eq!(json["location"]["order"], "last");
    }

    #[test]
    fn write_action_update_block_serializes() {
        let action = WriteAction::UpdateBlock {
            block: BlockUpdate {
                uid: "abc123".into(),
                string: "Updated content".into(),
            },
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["action"], "update-block");
        assert_eq!(json["block"]["uid"], "abc123");
    }

    #[test]
    fn write_action_delete_block_serializes() {
        let action = WriteAction::DeleteBlock {
            block: BlockRef {
                uid: "abc123".into(),
            },
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["action"], "delete-block");
        assert_eq!(json["block"]["uid"], "abc123");
    }

    #[test]
    fn write_action_move_block_serializes() {
        let action = WriteAction::MoveBlock {
            block: BlockRef {
                uid: "block1".into(),
            },
            location: BlockLocation {
                parent_uid: "new-parent".into(),
                order: OrderValue::Position("last".into()),
            },
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["action"], "move-block");
        assert_eq!(json["block"]["uid"], "block1");
        assert_eq!(json["location"]["parent-uid"], "new-parent");
        assert_eq!(json["location"]["order"], "last");
    }

    #[test]
    fn order_value_index_serializes_as_number() {
        let order = OrderValue::Index(5);
        let json = serde_json::to_value(&order).unwrap();
        assert_eq!(json, 5);
    }

    #[test]
    fn order_value_position_serializes_as_string() {
        let order = OrderValue::Position("last".into());
        let json = serde_json::to_value(&order).unwrap();
        assert_eq!(json, "last");
    }

    #[test]
    fn daily_note_from_pull_response_parses_blocks() {
        let pull_result = json!({
            ":node/title": "February 21, 2026",
            ":block/uid": "02-21-2026",
            ":block/children": [
                {
                    ":block/uid": "block2",
                    ":block/string": "Second block",
                    ":block/order": 1,
                    ":block/open": true
                },
                {
                    ":block/uid": "block1",
                    ":block/string": "First block",
                    ":block/order": 0,
                    ":block/open": true
                }
            ]
        });

        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 21).unwrap();
        let note = DailyNote::from_pull_response(date, "02-21-2026".into(), &pull_result);

        assert_eq!(note.title, "February 21, 2026");
        assert_eq!(note.uid, "02-21-2026");
        assert_eq!(note.date, date);
        assert_eq!(note.blocks.len(), 2);
        assert_eq!(note.blocks[0].string, "First block");
        assert_eq!(note.blocks[1].string, "Second block");
    }

    #[test]
    fn daily_note_from_pull_response_with_nested_children() {
        let pull_result = json!({
            ":node/title": "February 21, 2026",
            ":block/uid": "02-21-2026",
            ":block/children": [
                {
                    ":block/uid": "parent",
                    ":block/string": "Parent block",
                    ":block/order": 0,
                    ":block/open": true,
                    ":block/children": [
                        {
                            ":block/uid": "child2",
                            ":block/string": "Child B",
                            ":block/order": 1
                        },
                        {
                            ":block/uid": "child1",
                            ":block/string": "Child A",
                            ":block/order": 0
                        }
                    ]
                }
            ]
        });

        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 21).unwrap();
        let note = DailyNote::from_pull_response(date, "02-21-2026".into(), &pull_result);

        assert_eq!(note.blocks.len(), 1);
        assert_eq!(note.blocks[0].children.len(), 2);
        assert_eq!(note.blocks[0].children[0].string, "Child A");
        assert_eq!(note.blocks[0].children[1].string, "Child B");
    }

    #[test]
    fn daily_note_from_empty_pull_response() {
        let pull_result = json!({});
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 21).unwrap();
        let note = DailyNote::from_pull_response(date, "02-21-2026".into(), &pull_result);

        assert!(note.blocks.is_empty());
        assert_eq!(note.title, "");
        assert_eq!(note.blocks.len(), 0);
    }

    #[test]
    fn daily_note_from_pull_response_no_children() {
        let pull_result = json!({
            ":node/title": "February 21, 2026",
            ":block/uid": "02-21-2026"
        });
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 21).unwrap();
        let note = DailyNote::from_pull_response(date, "02-21-2026".into(), &pull_result);

        assert!(note.blocks.is_empty());
        assert_eq!(note.title, "February 21, 2026");
    }
}
