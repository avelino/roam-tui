use chrono::{Datelike, Local, NaiveDate};
use roam_sdk::api::client::RoamClient;
use roam_sdk::api::queries;
use roam_sdk::api::types::{
    generate_block_uid, parse_order, BlockLocation, BlockRef, BlockUpdate, NewBlock, PageCreate,
    WriteAction,
};

use crate::export;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// -- Read commands --

pub async fn get_page(client: &RoamClient, title: &str) -> Result<String> {
    let (eid, selector) = queries::pull_page_by_title(title);
    let resp = client.pull(eid, &selector).await?;
    Ok(serde_json::to_string_pretty(&resp.result)?)
}

pub async fn get_block(client: &RoamClient, uid: &str) -> Result<String> {
    let eid = serde_json::json!(["block/uid", uid]);
    let selector = "[:block/uid :block/string :block/order :block/open {:block/refs [:block/uid :node/title :block/string]} {:block/children ...}]";
    let resp = client.pull(eid, selector).await?;
    Ok(serde_json::to_string_pretty(&resp.result)?)
}

pub async fn get_daily(client: &RoamClient, date: Option<&str>) -> Result<String> {
    let (month, day, year) = match date {
        Some(date_str) => {
            let d = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
            (d.month(), d.day(), d.year())
        }
        None => {
            let today = Local::now().date_naive();
            (today.month(), today.day(), today.year())
        }
    };

    let uid = queries::daily_note_uid_for_date(month, day, year);
    let (eid, selector) = queries::pull_daily_note(&uid);
    let resp = client.pull(eid, &selector).await?;
    Ok(serde_json::to_string_pretty(&resp.result)?)
}

pub async fn get_backlinks(client: &RoamClient, title: &str) -> Result<String> {
    let query_str = queries::linked_refs_query(title);
    let resp = client.query(query_str, vec![]).await?;
    let groups = roam_sdk::types::parse_linked_refs(&resp.result, title);
    let json: Vec<serde_json::Value> = groups
        .iter()
        .map(|g| {
            serde_json::json!({
                "page_title": g.page_title,
                "blocks": g.blocks.iter().map(|b| {
                    serde_json::json!({"uid": b.uid, "string": b.string})
                }).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&json)?)
}

pub async fn get_refs(client: &RoamClient, uid: &str) -> Result<String> {
    let eid = serde_json::json!(["block/uid", uid]);
    let selector =
        "[:block/uid :block/string {:block/refs [:block/uid :node/title :block/string]}]";
    let resp = client.pull(eid, selector).await?;

    let refs = resp
        .result
        .get(":block/refs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    let uid = r.get(":block/uid")?.as_str()?;
                    let title = r.get(":node/title").and_then(|v| v.as_str());
                    let string = r.get(":block/string").and_then(|v| v.as_str());
                    Some(serde_json::json!({"uid": uid, "title": title, "string": string}))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(serde_json::to_string_pretty(&refs)?)
}

pub async fn get_stats(client: &RoamClient) -> Result<String> {
    let page_query = queries::graph_page_count_query();
    let block_query = queries::graph_block_count_query();

    let (page_resp, block_resp) = tokio::try_join!(
        client.query(page_query, vec![]),
        client.query(block_query, vec![]),
    )?;

    let page_count = page_resp
        .result
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let block_count = block_resp
        .result
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "pages": page_count,
        "blocks": block_count,
    }))?)
}

pub async fn search(
    client: &RoamClient,
    query: &str,
    blocks: bool,
    limit: Option<usize>,
) -> Result<String> {
    if blocks {
        let query_str = queries::search_blocks_query();
        let resp = client.query(query_str, vec![]).await?;
        let query_lower = query.to_lowercase();
        let limit = limit.unwrap_or(50);
        let matches: Vec<serde_json::Value> = resp
            .result
            .iter()
            .filter_map(|row| {
                let uid = row.first()?.as_str()?;
                let text = row.get(1)?.as_str()?;
                let page_title = row.get(2)?.as_str()?;
                if text.to_lowercase().contains(&query_lower) {
                    Some(serde_json::json!({"uid": uid, "string": text, "page_title": page_title}))
                } else {
                    None
                }
            })
            .take(limit)
            .collect();
        Ok(serde_json::to_string_pretty(&matches)?)
    } else {
        let query_str = queries::all_page_titles_query();
        let resp = client.query(query_str, vec![]).await?;
        let query_lower = query.to_lowercase();
        let mut matches: Vec<serde_json::Value> = resp
            .result
            .iter()
            .filter_map(|row| {
                let title = row.first()?.as_str()?;
                let uid = row.get(1)?.as_str()?;
                if title.to_lowercase().contains(&query_lower) {
                    Some(serde_json::json!({"title": title, "uid": uid}))
                } else {
                    None
                }
            })
            .collect();
        if let Some(limit) = limit {
            matches.truncate(limit);
        }
        Ok(serde_json::to_string_pretty(&matches)?)
    }
}

pub async fn query(client: &RoamClient, query_str: &str, args: Option<&str>) -> Result<String> {
    let args: Vec<serde_json::Value> = match args {
        Some(args_str) => serde_json::from_str(args_str)?,
        None => vec![],
    };
    let resp = client.query(query_str.to_string(), args).await?;
    Ok(serde_json::to_string_pretty(&resp.result)?)
}

// -- Journal commands --

pub async fn journal_view(client: &RoamClient, date: Option<&str>) -> Result<String> {
    get_daily(client, date).await
}

pub async fn journal_add(
    client: &RoamClient,
    text: &str,
    date: Option<&str>,
    order: Option<&str>,
    children: Option<&str>,
) -> Result<String> {
    let (month, day, year) = match date {
        Some(date_str) => {
            let d = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
            (d.month(), d.day(), d.year())
        }
        None => {
            let today = Local::now().date_naive();
            (today.month(), today.day(), today.year())
        }
    };

    let daily_uid = queries::daily_note_uid_for_date(month, day, year);
    let block_uid = generate_block_uid();

    let mut actions = vec![WriteAction::CreateBlock {
        location: BlockLocation {
            parent_uid: daily_uid,
            order: parse_order(order),
        },
        block: NewBlock {
            string: text.to_string(),
            uid: Some(block_uid.clone()),
            open: None,
        },
    }];

    if let Some(children_json) = children {
        let child_strings: Vec<String> = serde_json::from_str(children_json)?;
        for (i, child_text) in child_strings.iter().enumerate() {
            actions.push(WriteAction::CreateBlock {
                location: BlockLocation {
                    parent_uid: block_uid.clone(),
                    order: roam_sdk::api::types::OrderValue::Index(i as i64),
                },
                block: NewBlock {
                    string: child_text.clone(),
                    uid: None,
                    open: None,
                },
            });
        }
    }

    if actions.len() == 1 {
        client.write(actions.into_iter().next().unwrap()).await?;
    } else {
        client.write_batch(actions).await?;
    }

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "uid": block_uid,
        "status": "created",
    }))?)
}

// -- Write commands --

pub async fn create_page(client: &RoamClient, title: &str, uid: Option<&str>) -> Result<String> {
    let action = WriteAction::CreatePage {
        page: PageCreate {
            title: title.to_string(),
            uid: uid.map(|s| s.to_string()),
        },
    };
    client.write(action).await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "title": title,
        "status": "created",
    }))?)
}

pub async fn create_block(
    client: &RoamClient,
    parent_uid: &str,
    content: &str,
    order: Option<&str>,
    uid: Option<&str>,
    children: Option<&str>,
) -> Result<String> {
    let block_uid = uid
        .map(|s| s.to_string())
        .unwrap_or_else(generate_block_uid);

    let mut actions = vec![WriteAction::CreateBlock {
        location: BlockLocation {
            parent_uid: parent_uid.to_string(),
            order: parse_order(order),
        },
        block: NewBlock {
            string: content.to_string(),
            uid: Some(block_uid.clone()),
            open: None,
        },
    }];

    if let Some(children_json) = children {
        let child_strings: Vec<String> = serde_json::from_str(children_json)?;
        for (i, child_text) in child_strings.iter().enumerate() {
            actions.push(WriteAction::CreateBlock {
                location: BlockLocation {
                    parent_uid: block_uid.clone(),
                    order: roam_sdk::api::types::OrderValue::Index(i as i64),
                },
                block: NewBlock {
                    string: child_text.clone(),
                    uid: None,
                    open: None,
                },
            });
        }
    }

    if actions.len() == 1 {
        client.write(actions.into_iter().next().unwrap()).await?;
    } else {
        client.write_batch(actions).await?;
    }

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "uid": block_uid,
        "status": "created",
    }))?)
}

pub async fn update_block(client: &RoamClient, uid: &str, content: &str) -> Result<String> {
    let action = WriteAction::UpdateBlock {
        block: BlockUpdate {
            uid: uid.to_string(),
            string: content.to_string(),
        },
    };
    client.write(action).await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "uid": uid,
        "status": "updated",
    }))?)
}

pub async fn delete_block(client: &RoamClient, uid: &str) -> Result<String> {
    let action = WriteAction::DeleteBlock {
        block: BlockRef {
            uid: uid.to_string(),
        },
    };
    client.write(action).await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "uid": uid,
        "status": "deleted",
    }))?)
}

pub async fn delete_page(client: &RoamClient, uid: &str) -> Result<String> {
    let action = WriteAction::DeleteBlock {
        block: BlockRef {
            uid: uid.to_string(),
        },
    };
    client.write(action).await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "uid": uid,
        "status": "deleted",
    }))?)
}

pub async fn move_block(
    client: &RoamClient,
    uid: &str,
    parent_uid: &str,
    order: Option<&str>,
) -> Result<String> {
    let action = WriteAction::MoveBlock {
        block: BlockRef {
            uid: uid.to_string(),
        },
        location: BlockLocation {
            parent_uid: parent_uid.to_string(),
            order: parse_order(order),
        },
    };
    client.write(action).await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "uid": uid,
        "status": "moved",
    }))?)
}

pub async fn batch(client: &RoamClient, input: &str) -> Result<String> {
    let actions: Vec<WriteAction> = serde_json::from_str(input)?;
    let count = actions.len();
    client.write_batch(actions).await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "actions": count,
        "status": "executed",
    }))?)
}

// -- Export (moved from main.rs) --

pub async fn run_export(
    client: &RoamClient,
    date: Option<&str>,
    page: Option<&str>,
    format: &str,
    output: Option<&str>,
) -> Result<()> {
    use roam_sdk::api::types::DailyNote;

    let content = if let Some(title) = page {
        let (eid, selector) = queries::pull_page_by_title(title);
        let resp = client.pull(eid, &selector).await?;
        let note =
            DailyNote::from_pull_response(Local::now().date_naive(), "".into(), &resp.result);

        match format {
            "json" => export::blocks_to_json(title, &note.blocks),
            _ => export::blocks_to_markdown(title, &note.blocks),
        }
    } else {
        let date = match date {
            Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d")?,
            None => Local::now().date_naive(),
        };
        let uid = queries::daily_note_uid_for_date(date.month(), date.day(), date.year());
        let (eid, selector) = queries::pull_daily_note(&uid);
        let resp = client.pull(eid, &selector).await?;
        let note = DailyNote::from_pull_response(date, uid, &resp.result);

        match format {
            "json" => export::daily_notes_to_json(&[note]),
            _ => export::daily_notes_to_markdown(&[note]),
        }
    };

    if let Some(path) = output {
        std::fs::write(path, &content)?;
        eprintln!("Exported to {}", path);
    } else {
        print!("{}", content);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup() -> (MockServer, RoamClient) {
        let server = MockServer::start().await;
        let client = RoamClient::new_with_base_url(&server.uri(), "test-token");
        (server, client)
    }

    fn mock_pull(result: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({"result": result}))
    }

    fn mock_query(result: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({"result": result}))
    }

    fn mock_write_ok() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({}))
    }

    // -- get_page --

    #[tokio::test]
    async fn get_page_returns_json() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/pull"))
            .and(header("X-Authorization", "Bearer test-token"))
            .respond_with(mock_pull(json!({
                ":node/title": "Test",
                ":block/uid": "p1",
                ":block/children": [
                    {":block/uid": "b1", ":block/string": "Hello", ":block/order": 0}
                ]
            })))
            .mount(&server)
            .await;

        let result = get_page(&client, "Test").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[":node/title"], "Test");
        assert!(parsed[":block/children"].is_array());
    }

    #[tokio::test]
    async fn get_page_api_error() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let result = get_page(&client, "Missing").await;
        assert!(result.is_err());
    }

    // -- get_block --

    #[tokio::test]
    async fn get_block_returns_json() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(mock_pull(json!({
                ":block/uid": "abc",
                ":block/string": "Content",
                ":block/order": 0
            })))
            .mount(&server)
            .await;

        let result = get_block(&client, "abc").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[":block/uid"], "abc");
    }

    // -- get_daily --

    #[tokio::test]
    async fn get_daily_with_date() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(mock_pull(json!({
                ":node/title": "March 15, 2026",
                ":block/uid": "03-15-2026"
            })))
            .mount(&server)
            .await;

        let result = get_daily(&client, Some("2026-03-15")).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[":node/title"], "March 15, 2026");
    }

    #[tokio::test]
    async fn get_daily_today() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(mock_pull(json!({":node/title": "Today"})))
            .mount(&server)
            .await;

        let result = get_daily(&client, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_daily_invalid_date() {
        let (_server, client) = setup().await;
        let result = get_daily(&client, Some("not-a-date")).await;
        assert!(result.is_err());
    }

    // -- get_backlinks --

    #[tokio::test]
    async fn get_backlinks_groups_by_page() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([
                ["b1", "ref [[Target]]", "Page A"],
                ["b2", "also [[Target]]", "Page B"]
            ])))
            .mount(&server)
            .await;

        let result = get_backlinks(&client, "Target").await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["page_title"], "Page A");
    }

    // -- get_refs --

    #[tokio::test]
    async fn get_refs_returns_references() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(mock_pull(json!({
                ":block/uid": "b1",
                ":block/string": "Links to [[PageA]]",
                ":block/refs": [
                    {":block/uid": "pa-uid", ":node/title": "PageA"}
                ]
            })))
            .mount(&server)
            .await;

        let result = get_refs(&client, "b1").await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["uid"], "pa-uid");
        assert_eq!(parsed[0]["title"], "PageA");
    }

    #[tokio::test]
    async fn get_refs_empty_when_no_refs() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(mock_pull(json!({
                ":block/uid": "b1",
                ":block/string": "No links"
            })))
            .mount(&server)
            .await;

        let result = get_refs(&client, "b1").await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_empty());
    }

    // -- get_stats --

    #[tokio::test]
    async fn get_stats_returns_counts() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([[42]])))
            .mount(&server)
            .await;

        let result = get_stats(&client).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["pages"], 42);
        assert_eq!(parsed["blocks"], 42);
    }

    // -- search --

    #[tokio::test]
    async fn search_pages_filters_by_title() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([
                ["Project Alpha", "pa"],
                ["Project Beta", "pb"],
                ["Random", "r"]
            ])))
            .mount(&server)
            .await;

        let result = search(&client, "project", false, None).await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["title"], "Project Alpha");
    }

    #[tokio::test]
    async fn search_pages_with_limit() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([["A", "a"], ["B", "b"], ["C", "c"]])))
            .mount(&server)
            .await;

        let result = search(&client, "", false, Some(2)).await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[tokio::test]
    async fn search_blocks_filters_by_content() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([
                ["b1", "Hello world", "Page A"],
                ["b2", "Goodbye", "Page B"],
                ["b3", "Hello again", "Page C"]
            ])))
            .mount(&server)
            .await;

        let result = search(&client, "hello", true, None).await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["uid"], "b1");
        assert_eq!(parsed[0]["page_title"], "Page A");
    }

    #[tokio::test]
    async fn search_blocks_with_limit() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([
                ["b1", "match 1", "P1"],
                ["b2", "match 2", "P2"],
                ["b3", "match 3", "P3"]
            ])))
            .mount(&server)
            .await;

        let result = search(&client, "match", true, Some(2)).await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    // -- query --

    #[tokio::test]
    async fn query_raw_datalog() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([["uid1", "text"]])))
            .mount(&server)
            .await;

        let result = query(
            &client,
            "[:find ?uid ?s :where [?b :block/uid ?uid] [?b :block/string ?s]]",
            None,
        )
        .await
        .unwrap();
        let parsed: Vec<Vec<serde_json::Value>> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[tokio::test]
    async fn query_with_args() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(mock_query(json!([["uid1"]])))
            .mount(&server)
            .await;

        let result = query(
            &client,
            "[:find ?uid :in $ ?title :where [?e :node/title ?title] [?e :block/uid ?uid]]",
            Some(r#"["My Page"]"#),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn query_invalid_args() {
        let (_server, client) = setup().await;
        let result = query(
            &client,
            "[:find ?b :where [?b :block/string]]",
            Some("bad json"),
        )
        .await;
        assert!(result.is_err());
    }

    // -- journal_add --

    #[tokio::test]
    async fn journal_add_creates_block() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = journal_add(&client, "Test entry", None, None, None)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "created");
        assert!(parsed["uid"].as_str().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn journal_add_with_children() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .expect(2) // parent + child each as separate write
            .mount(&server)
            .await;

        let result = journal_add(
            &client,
            "Parent",
            Some("2026-03-15"),
            Some("first"),
            Some(r#"["Child 1"]"#),
        )
        .await
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "created");
    }

    // -- create_page --

    #[tokio::test]
    async fn create_page_sends_write() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = create_page(&client, "New Page", None).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "New Page");
        assert_eq!(parsed["status"], "created");
    }

    #[tokio::test]
    async fn create_page_with_uid() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = create_page(&client, "Custom", Some("my-uid"))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "created");
    }

    // -- create_block --

    #[tokio::test]
    async fn create_block_sends_write() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = create_block(&client, "parent", "Hello", None, None, None)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "created");
    }

    #[tokio::test]
    async fn create_block_with_children() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = create_block(
            &client,
            "parent",
            "Parent block",
            Some("first"),
            Some("custom-uid"),
            Some(r#"["Child A", "Child B"]"#),
        )
        .await
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["uid"], "custom-uid");
        assert_eq!(parsed["status"], "created");
    }

    // -- update_block --

    #[tokio::test]
    async fn update_block_sends_write() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = update_block(&client, "b1", "Updated").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["uid"], "b1");
        assert_eq!(parsed["status"], "updated");
    }

    // -- delete_block --

    #[tokio::test]
    async fn delete_block_sends_write() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = delete_block(&client, "b1").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["uid"], "b1");
        assert_eq!(parsed["status"], "deleted");
    }

    // -- delete_page --

    #[tokio::test]
    async fn delete_page_sends_write() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = delete_page(&client, "p1").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["uid"], "p1");
        assert_eq!(parsed["status"], "deleted");
    }

    // -- move_block --

    #[tokio::test]
    async fn move_block_sends_write() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .mount(&server)
            .await;

        let result = move_block(&client, "b1", "new-parent", Some("2"))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["uid"], "b1");
        assert_eq!(parsed["status"], "moved");
    }

    // -- batch --

    #[tokio::test]
    async fn batch_executes_actions() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(mock_write_ok())
            .expect(2)
            .mount(&server)
            .await;

        let input = json!([
            {"action": "create-page", "page": {"title": "Batch Page"}},
            {"action": "update-block", "block": {"uid": "b1", "string": "Updated"}}
        ])
        .to_string();

        let result = batch(&client, &input).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["actions"], 2);
        assert_eq!(parsed["status"], "executed");
    }

    #[tokio::test]
    async fn batch_invalid_json() {
        let (_server, client) = setup().await;
        let result = batch(&client, "not json").await;
        assert!(result.is_err());
    }

    // -- write error propagation --

    #[tokio::test]
    async fn write_error_propagates() {
        let (server, client) = setup().await;
        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Server Error"))
            .mount(&server)
            .await;

        assert!(create_page(&client, "Fail", None).await.is_err());
        assert!(update_block(&client, "x", "y").await.is_err());
        assert!(delete_block(&client, "x").await.is_err());
        assert!(move_block(&client, "x", "y", None).await.is_err());
    }
}
