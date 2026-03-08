use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use roam_sdk::api::client::RoamClient;
use roam_sdk::api::queries;
use roam_sdk::api::types::{
    BlockLocation, BlockRef, BlockUpdate, NewBlock, OrderValue, PageCreate, WriteAction,
};
use serde::Deserialize;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct RoamMcp {
    client: RoamClient,
    tool_router: ToolRouter<Self>,
}

impl RoamMcp {
    pub fn new(client: RoamClient) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
        }
    }
}

fn parse_order(order: Option<String>) -> OrderValue {
    match order.as_deref() {
        None | Some("last") => OrderValue::Position("last".into()),
        Some("first") => OrderValue::Position("first".into()),
        Some(n) => n
            .parse::<i64>()
            .map(OrderValue::Index)
            .unwrap_or(OrderValue::Position("last".into())),
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchParams {
    /// Search query to filter page titles (case-insensitive substring match)
    query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetPageParams {
    /// Page title to retrieve
    title: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetBlockParams {
    /// Block UID to retrieve
    uid: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetBacklinksParams {
    /// Page title to find backlinks for
    title: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RoamQueryParams {
    /// Datalog query string
    query: String,
    /// JSON array of query arguments (optional)
    #[schemars(description = "JSON array of query arguments")]
    args: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreatePageParams {
    /// Title for the new page
    title: String,
    /// Optional UID for the page
    uid: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateBlockParams {
    /// UID of the parent block or page
    parent_uid: String,
    /// Block content (Roam markdown)
    content: String,
    /// Position: "first", "last", or numeric index
    order: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateBlockParams {
    /// Block UID to update
    uid: String,
    /// New content for the block
    content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteBlockParams {
    /// Block UID to delete
    uid: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeletePageParams {
    /// Page UID to delete
    uid: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MoveBlockParams {
    /// Block UID to move
    uid: String,
    /// UID of the new parent
    parent_uid: String,
    /// Position: "first", "last", or numeric index
    order: Option<String>,
}

#[tool_router]
impl RoamMcp {
    #[tool(description = "Search pages by title. Returns matching page titles and UIDs.")]
    async fn search(&self, Parameters(params): Parameters<SearchParams>) -> Result<String, String> {
        let query_str = queries::all_page_titles_query();
        let resp = self
            .client
            .query(query_str, vec![])
            .await
            .map_err(|e| e.to_string())?;

        let query_lower = params.query.to_lowercase();
        let matches: Vec<serde_json::Value> = resp
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

        serde_json::to_string_pretty(&matches).map_err(|e| e.to_string())
    }

    #[tool(description = "Get a page by title with all its blocks and children.")]
    async fn get_page(
        &self,
        Parameters(params): Parameters<GetPageParams>,
    ) -> Result<String, String> {
        let (eid, selector) = queries::pull_page_by_title(&params.title);
        let resp = self
            .client
            .pull(eid, &selector)
            .await
            .map_err(|e| e.to_string())?;

        serde_json::to_string_pretty(&resp.result).map_err(|e| e.to_string())
    }

    #[tool(description = "Get a block by UID with its children.")]
    async fn get_block(
        &self,
        Parameters(params): Parameters<GetBlockParams>,
    ) -> Result<String, String> {
        let eid = serde_json::json!(["block/uid", params.uid]);
        let selector = "[:block/uid :block/string :block/order :block/open {:block/refs [:block/uid :node/title :block/string]} {:block/children ...}]";
        let resp = self
            .client
            .pull(eid, selector)
            .await
            .map_err(|e| e.to_string())?;

        serde_json::to_string_pretty(&resp.result).map_err(|e| e.to_string())
    }

    #[tool(description = "Get backlinks (blocks that reference a page by title).")]
    async fn get_backlinks(
        &self,
        Parameters(params): Parameters<GetBacklinksParams>,
    ) -> Result<String, String> {
        let query_str = queries::linked_refs_query(&params.title);
        let resp = self
            .client
            .query(query_str, vec![])
            .await
            .map_err(|e| e.to_string())?;

        let groups = roam_sdk::types::parse_linked_refs(&resp.result, &params.title);
        serde_json::to_string_pretty(&serde_json::json!(groups
            .iter()
            .map(|g| {
                serde_json::json!({
                    "page_title": g.page_title,
                    "blocks": g.blocks.iter().map(|b| {
                        serde_json::json!({
                            "uid": b.uid,
                            "string": b.string,
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>()))
        .map_err(|e| e.to_string())
    }

    #[tool(description = "Run a raw Datalog query against the Roam graph.")]
    async fn roam_query(
        &self,
        Parameters(params): Parameters<RoamQueryParams>,
    ) -> Result<String, String> {
        let args: Vec<serde_json::Value> = match &params.args {
            Some(args_str) => {
                serde_json::from_str(args_str).map_err(|e| format!("Invalid args JSON: {}", e))?
            }
            None => vec![],
        };

        let resp = self
            .client
            .query(params.query, args)
            .await
            .map_err(|e| e.to_string())?;

        serde_json::to_string_pretty(&resp.result).map_err(|e| e.to_string())
    }

    #[tool(description = "Create a new page in the Roam graph.")]
    async fn create_page(
        &self,
        Parameters(params): Parameters<CreatePageParams>,
    ) -> Result<String, String> {
        let action = WriteAction::CreatePage {
            page: PageCreate {
                title: params.title.clone(),
                uid: params.uid,
            },
        };

        self.client.write(action).await.map_err(|e| e.to_string())?;

        Ok(format!("Created page '{}'", params.title))
    }

    #[tool(description = "Create a new block under a parent block or page.")]
    async fn create_block(
        &self,
        Parameters(params): Parameters<CreateBlockParams>,
    ) -> Result<String, String> {
        let action = WriteAction::CreateBlock {
            location: BlockLocation {
                parent_uid: params.parent_uid,
                order: parse_order(params.order),
            },
            block: NewBlock {
                string: params.content,
                uid: None,
                open: None,
            },
        };

        self.client.write(action).await.map_err(|e| e.to_string())?;

        Ok("Block created".into())
    }

    #[tool(description = "Update the content of an existing block.")]
    async fn update_block(
        &self,
        Parameters(params): Parameters<UpdateBlockParams>,
    ) -> Result<String, String> {
        let action = WriteAction::UpdateBlock {
            block: BlockUpdate {
                uid: params.uid.clone(),
                string: params.content,
            },
        };

        self.client.write(action).await.map_err(|e| e.to_string())?;

        Ok(format!("Updated block '{}'", params.uid))
    }

    #[tool(description = "Delete a block by UID.")]
    async fn delete_block(
        &self,
        Parameters(params): Parameters<DeleteBlockParams>,
    ) -> Result<String, String> {
        let action = WriteAction::DeleteBlock {
            block: BlockRef {
                uid: params.uid.clone(),
            },
        };

        self.client.write(action).await.map_err(|e| e.to_string())?;

        Ok(format!("Deleted block '{}'", params.uid))
    }

    #[tool(description = "Delete a page by UID.")]
    async fn delete_page(
        &self,
        Parameters(params): Parameters<DeletePageParams>,
    ) -> Result<String, String> {
        let action = WriteAction::DeleteBlock {
            block: BlockRef {
                uid: params.uid.clone(),
            },
        };

        self.client.write(action).await.map_err(|e| e.to_string())?;

        Ok(format!("Deleted page '{}'", params.uid))
    }

    #[tool(description = "Move a block to a new parent.")]
    async fn move_block(
        &self,
        Parameters(params): Parameters<MoveBlockParams>,
    ) -> Result<String, String> {
        let action = WriteAction::MoveBlock {
            block: BlockRef {
                uid: params.uid.clone(),
            },
            location: BlockLocation {
                parent_uid: params.parent_uid,
                order: parse_order(params.order),
            },
        };

        self.client.write(action).await.map_err(|e| e.to_string())?;

        Ok(format!("Moved block '{}'", params.uid))
    }
}

#[tool_handler]
impl ServerHandler for RoamMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Roam Research MCP server — read and write to your Roam graph via the cloud API",
        )
    }
}

pub async fn run(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let client = RoamClient::new(&config.graph.name, &config.graph.api_token);
    let service = RoamMcp::new(client);
    let server = service.serve(rmcp::transport::io::stdio()).await?;
    server.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup() -> (MockServer, RoamMcp) {
        let server = MockServer::start().await;
        let client = RoamClient::new_with_base_url(&server.uri(), "test-token");
        (server, RoamMcp::new(client))
    }

    // -- parse_order tests --

    #[test]
    fn parse_order_none_defaults_to_last() {
        match parse_order(None) {
            OrderValue::Position(s) => assert_eq!(s, "last"),
            _ => panic!("Expected Position"),
        }
    }

    #[test]
    fn parse_order_last() {
        match parse_order(Some("last".into())) {
            OrderValue::Position(s) => assert_eq!(s, "last"),
            _ => panic!("Expected Position"),
        }
    }

    #[test]
    fn parse_order_first() {
        match parse_order(Some("first".into())) {
            OrderValue::Position(s) => assert_eq!(s, "first"),
            _ => panic!("Expected Position"),
        }
    }

    #[test]
    fn parse_order_numeric() {
        match parse_order(Some("3".into())) {
            OrderValue::Index(n) => assert_eq!(n, 3),
            _ => panic!("Expected Index"),
        }
    }

    #[test]
    fn parse_order_invalid_defaults_to_last() {
        match parse_order(Some("invalid".into())) {
            OrderValue::Position(s) => assert_eq!(s, "last"),
            _ => panic!("Expected Position"),
        }
    }

    // -- search tool --

    #[tokio::test]
    async fn search_filters_pages_by_query() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .and(header("X-Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": [
                    ["Daily Notes", "dn-uid"],
                    ["Project Alpha", "pa-uid"],
                    ["Project Beta", "pb-uid"],
                    ["Random Page", "rp-uid"]
                ]
            })))
            .mount(&server)
            .await;

        let result = mcp
            .search(Parameters(SearchParams {
                query: "project".into(),
            }))
            .await;

        let result = result.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["title"], "Project Alpha");
        assert_eq!(parsed[1]["title"], "Project Beta");
    }

    #[tokio::test]
    async fn search_returns_error_on_api_failure() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let result = mcp
            .search(Parameters(SearchParams {
                query: "test".into(),
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("401"));
    }

    // -- get_page tool --

    #[tokio::test]
    async fn get_page_calls_pull_endpoint() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .and(header("X-Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    ":node/title": "My Page",
                    ":block/uid": "page-uid",
                    ":block/children": [{
                        ":block/uid": "b1",
                        ":block/string": "Hello",
                        ":block/order": 0
                    }]
                }
            })))
            .mount(&server)
            .await;

        let result = mcp
            .get_page(Parameters(GetPageParams {
                title: "My Page".into(),
            }))
            .await;

        let result = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[":node/title"], "My Page");
    }

    // -- get_block tool --

    #[tokio::test]
    async fn get_block_calls_pull_with_uid() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    ":block/uid": "abc123",
                    ":block/string": "Block content",
                    ":block/order": 0
                }
            })))
            .mount(&server)
            .await;

        let result = mcp
            .get_block(Parameters(GetBlockParams {
                uid: "abc123".into(),
            }))
            .await;

        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed[":block/uid"], "abc123");
    }

    // -- get_backlinks tool --

    #[tokio::test]
    async fn get_backlinks_returns_grouped_refs() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": [
                    ["b1", "mentions [[Target]]", "Page A"],
                    ["b2", "also refs [[Target]]", "Page B"]
                ]
            })))
            .mount(&server)
            .await;

        let result = mcp
            .get_backlinks(Parameters(GetBacklinksParams {
                title: "Target".into(),
            }))
            .await;

        let result = result.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["page_title"], "Page A");
        assert_eq!(parsed[1]["page_title"], "Page B");
    }

    // -- roam_query tool --

    #[tokio::test]
    async fn roam_query_passes_raw_query() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"result": [["uid1", "text"]]})),
            )
            .mount(&server)
            .await;

        let result = mcp
            .roam_query(Parameters(RoamQueryParams {
                query: "[:find ?uid ?s :where [?b :block/uid ?uid] [?b :block/string ?s]]".into(),
                args: None,
            }))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn roam_query_with_args() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"result": [["uid1"]]})))
            .mount(&server)
            .await;

        let result = mcp
            .roam_query(Parameters(RoamQueryParams {
                query:
                    "[:find ?uid :in $ ?title :where [?e :node/title ?title] [?e :block/uid ?uid]]"
                        .into(),
                args: Some(r#"["My Page"]"#.into()),
            }))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn roam_query_invalid_args_returns_error() {
        let (_server, mcp) = setup().await;

        let result = mcp
            .roam_query(Parameters(RoamQueryParams {
                query: "[:find ?b :where [?b :block/string]]".into(),
                args: Some("not valid json".into()),
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid args JSON"));
    }

    // -- create_page tool --

    #[tokio::test]
    async fn create_page_calls_write_endpoint() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .and(header("X-Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .create_page(Parameters(CreatePageParams {
                title: "New Page".into(),
                uid: None,
            }))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("New Page"));
    }

    // -- create_block tool --

    #[tokio::test]
    async fn create_block_calls_write_endpoint() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .create_block(Parameters(CreateBlockParams {
                parent_uid: "parent-uid".into(),
                content: "Block content".into(),
                order: Some("first".into()),
            }))
            .await;

        assert!(result.is_ok());
    }

    // -- update_block tool --

    #[tokio::test]
    async fn update_block_calls_write_endpoint() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .update_block(Parameters(UpdateBlockParams {
                uid: "block-uid".into(),
                content: "Updated content".into(),
            }))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("block-uid"));
    }

    // -- delete_block tool --

    #[tokio::test]
    async fn delete_block_calls_write_endpoint() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .delete_block(Parameters(DeleteBlockParams {
                uid: "del-uid".into(),
            }))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("del-uid"));
    }

    // -- delete_page tool --

    #[tokio::test]
    async fn delete_page_calls_write_endpoint() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .delete_page(Parameters(DeletePageParams {
                uid: "page-uid".into(),
            }))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("page-uid"));
    }

    // -- move_block tool --

    #[tokio::test]
    async fn move_block_calls_write_endpoint() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .move_block(Parameters(MoveBlockParams {
                uid: "block-uid".into(),
                parent_uid: "new-parent".into(),
                order: Some("2".into()),
            }))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("block-uid"));
    }

    // -- error handling --

    #[tokio::test]
    async fn write_error_500_returns_tool_error() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let result = mcp
            .create_page(Parameters(CreatePageParams {
                title: "Fail Page".into(),
                uid: None,
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("500"));
    }
}
