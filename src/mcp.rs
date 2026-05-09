use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use roam_sdk::api::client::RoamClient;
use roam_sdk::api::queries;
use serde::Deserialize;

use crate::commands;
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

// -- Parameter structs (required by rmcp #[tool] macro) --

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchParams {
    /// Search query to filter page titles (case-insensitive substring match)
    query: String,
    /// Maximum number of results to return (default: no limit)
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchBlocksParams {
    /// Search query to find in block content (case-insensitive substring match)
    query: String,
    /// Maximum number of results to return (default: 50)
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetDailyNoteParams {
    /// Date in YYYY-MM-DD format (e.g. "2026-03-15"). If omitted, returns today's daily note.
    date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportPageAsMarkdownParams {
    /// Page title to export as markdown
    title: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetBlockRefsParams {
    /// Block UID to get outbound references for
    uid: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct BatchWriteParams {
    /// JSON array of write actions. Each action is an object with "action" field ("create-block", "update-block", "delete-block", "move-block", "create-page") and corresponding fields. Example: [{"action": "create-block", "location": {"parent-uid": "abc", "order": "last"}, "block": {"string": "Hello"}}, {"action": "update-block", "block": {"uid": "xyz", "string": "Updated"}}]
    actions: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateBlockWithChildrenParams {
    /// UID of the parent block or page
    parent_uid: String,
    /// Block content (Roam markdown)
    content: String,
    /// Position: "first", "last", or numeric index
    order: Option<String>,
    /// Optional UID for the new block (auto-generated if omitted)
    uid: Option<String>,
    /// JSON array of child block strings to create nested under this block. Example: ["child 1", "child 2"]
    children: Option<String>,
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

// -- Tool handlers (delegate to commands module) --

#[tool_router]
impl RoamMcp {
    #[tool(
        description = "Search pages by title in the Roam graph. Returns matching page titles and UIDs. Use this to find pages before retrieving their content with get_page. For searching inside block content, use search_blocks instead."
    )]
    async fn search(&self, Parameters(params): Parameters<SearchParams>) -> Result<String, String> {
        commands::search(&self.client, &params.query, false, params.limit)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Get a page by title with its full block tree (children, refs, order). Returns the raw Roam pull result with :node/title, :block/uid, :block/children, :block/string, :block/refs. Use search first to find the exact page title."
    )]
    async fn get_page(
        &self,
        Parameters(params): Parameters<GetPageParams>,
    ) -> Result<String, String> {
        commands::get_page(&self.client, &params.title)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Get a block by UID with its full subtree (children, refs, order, open/collapsed state). Use search or get_page first to find block UIDs."
    )]
    async fn get_block(
        &self,
        Parameters(params): Parameters<GetBlockParams>,
    ) -> Result<String, String> {
        commands::get_block(&self.client, &params.uid)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Get backlinks — all blocks across the graph that reference a page by title. Returns results grouped by source page. Useful for discovering connections and seeing how a concept is used across your notes."
    )]
    async fn get_backlinks(
        &self,
        Parameters(params): Parameters<GetBacklinksParams>,
    ) -> Result<String, String> {
        commands::get_backlinks(&self.client, &params.title)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Run a raw Datalog query against the Roam graph. Example queries: find all pages: '[:find ?title ?uid :where [?e :node/title ?title] [?e :block/uid ?uid]]', find blocks containing text: '[:find ?uid ?s :in $ ?search :where [?b :block/string ?s] [?b :block/uid ?uid] [(clojure.string/includes? ?s ?search)]]' with args '[\"search term\"]'. Returns a 2D array of results."
    )]
    async fn roam_query(
        &self,
        Parameters(params): Parameters<RoamQueryParams>,
    ) -> Result<String, String> {
        commands::query(&self.client, &params.query, params.args.as_deref())
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Create a new page in the Roam graph. The page will be empty — use create_block to add content to it afterwards. To create a page with initial content, use batch_write instead."
    )]
    async fn create_page(
        &self,
        Parameters(params): Parameters<CreatePageParams>,
    ) -> Result<String, String> {
        commands::create_page(&self.client, &params.title, params.uid.as_deref())
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Create a new block under a parent block or page. For creating a block with nested children in one call, use create_block_with_children. For creating multiple blocks at once, use batch_write."
    )]
    async fn create_block(
        &self,
        Parameters(params): Parameters<CreateBlockParams>,
    ) -> Result<String, String> {
        commands::create_block(
            &self.client,
            &params.parent_uid,
            &params.content,
            params.order.as_deref(),
            None,
            None,
        )
        .await
        .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Update the text content of an existing block. Use get_block or get_page first to find the block UID. For updating multiple blocks at once, use batch_write."
    )]
    async fn update_block(
        &self,
        Parameters(params): Parameters<UpdateBlockParams>,
    ) -> Result<String, String> {
        commands::update_block(&self.client, &params.uid, &params.content)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Delete a block by UID. This removes the block and all its children permanently. For deleting multiple blocks at once, use batch_write."
    )]
    async fn delete_block(
        &self,
        Parameters(params): Parameters<DeleteBlockParams>,
    ) -> Result<String, String> {
        commands::delete_block(&self.client, &params.uid)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Delete a page by UID. This removes the page and all its blocks permanently. Use search to find the page UID first."
    )]
    async fn delete_page(
        &self,
        Parameters(params): Parameters<DeletePageParams>,
    ) -> Result<String, String> {
        commands::delete_page(&self.client, &params.uid)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Move a block to a new parent block or page. The block keeps its content and children. Use get_page or get_block to find UIDs."
    )]
    async fn move_block(
        &self,
        Parameters(params): Parameters<MoveBlockParams>,
    ) -> Result<String, String> {
        commands::move_block(
            &self.client,
            &params.uid,
            &params.parent_uid,
            params.order.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Get a daily note by date. Returns the full block tree for that day's note. If no date is provided, returns today's daily note. Useful for reviewing what was written on a specific day or adding content to today's journal."
    )]
    async fn get_daily_note(
        &self,
        Parameters(params): Parameters<GetDailyNoteParams>,
    ) -> Result<String, String> {
        commands::get_daily(&self.client, params.date.as_deref())
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Search inside block content across the entire graph (full-text search). Returns matching blocks with their UID, text, and the page they belong to. Use this when you need to find specific text inside blocks, not just page titles."
    )]
    async fn search_blocks(
        &self,
        Parameters(params): Parameters<SearchBlocksParams>,
    ) -> Result<String, String> {
        commands::search(&self.client, &params.query, true, params.limit)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Execute multiple write operations in sequence. Each action is sent as a separate API request, stopping on the first error. Accepts a JSON array of actions. Supported actions: create-block, update-block, delete-block, move-block, create-page. Example: [{\"action\": \"create-page\", \"page\": {\"title\": \"New Page\"}}, {\"action\": \"create-block\", \"location\": {\"parent-uid\": \"page-uid\", \"order\": \"last\"}, \"block\": {\"string\": \"Content\"}}]"
    )]
    async fn batch_write(
        &self,
        Parameters(params): Parameters<BatchWriteParams>,
    ) -> Result<String, String> {
        commands::batch(&self.client, &params.actions)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Export a page as formatted markdown. Converts the block tree into indented markdown with proper nesting. Useful for reading page content in a clean format or processing it externally."
    )]
    async fn export_page_as_markdown(
        &self,
        Parameters(params): Parameters<ExportPageAsMarkdownParams>,
    ) -> Result<String, String> {
        let (eid, selector) = queries::pull_page_by_title(&params.title);
        let resp = self
            .client
            .pull(eid, &selector)
            .await
            .map_err(|e| e.to_string())?;

        let title = resp
            .result
            .get(":node/title")
            .and_then(|v| v.as_str())
            .unwrap_or(&params.title);

        let mut output = format!("# {}\n\n", title);
        if let Some(children) = resp
            .result
            .get(":block/children")
            .and_then(|v| v.as_array())
        {
            let mut sorted = children.clone();
            sorted.sort_by_key(|b| b.get(":block/order").and_then(|v| v.as_i64()).unwrap_or(0));
            for child in &sorted {
                render_block_as_markdown(child, 0, &mut output);
            }
        }

        Ok(output)
    }

    #[tool(
        description = "Get all outbound references from a block — pages and blocks that this block links to via [[page refs]] or ((block refs)). Returns a list of referenced entities with their UIDs and titles."
    )]
    async fn get_block_refs(
        &self,
        Parameters(params): Parameters<GetBlockRefsParams>,
    ) -> Result<String, String> {
        commands::get_refs(&self.client, &params.uid)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Get graph statistics — total number of pages and blocks. Useful for understanding the size and scope of the Roam graph at a glance."
    )]
    async fn get_graph_stats(&self) -> Result<String, String> {
        commands::get_stats(&self.client)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Create a block with optional nested children in a single operation. More efficient than calling create_block multiple times. Each child is created in order under the new parent block."
    )]
    async fn create_block_with_children(
        &self,
        Parameters(params): Parameters<CreateBlockWithChildrenParams>,
    ) -> Result<String, String> {
        commands::create_block(
            &self.client,
            &params.parent_uid,
            &params.content,
            params.order.as_deref(),
            params.uid.as_deref(),
            params.children.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
    }
}

fn render_block_as_markdown(block: &serde_json::Value, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    let text = block
        .get(":block/string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    output.push_str(&format!("{}- {}\n", indent, text));

    if let Some(children) = block.get(":block/children").and_then(|v| v.as_array()) {
        let mut sorted = children.clone();
        sorted.sort_by_key(|b| b.get(":block/order").and_then(|v| v.as_i64()).unwrap_or(0));
        for child in &sorted {
            render_block_as_markdown(child, depth + 1, output);
        }
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
    let client = if config.graph.local_api {
        let base_url = format!(
            "http://localhost:{}/api/graph/{}",
            config.graph.local_api_port, config.graph.name
        );
        RoamClient::new_with_base_url(&base_url, &config.graph.api_token)
    } else {
        RoamClient::new(&config.graph.name, &config.graph.api_token)
    };
    let service = RoamMcp::new(client);
    let server = service.serve(rmcp::transport::io::stdio()).await?;
    server.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use roam_sdk::api::types::{BlockUpdate, OrderValue, PageCreate, WriteAction};
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup() -> (MockServer, RoamMcp) {
        let server = MockServer::start().await;
        let client = RoamClient::new_with_base_url(&server.uri(), "test-token");
        (server, RoamMcp::new(client))
    }

    // -- parse_order tests (from types module, kept for coverage) --

    #[test]
    fn parse_order_none_defaults_to_last() {
        use roam_sdk::api::types::parse_order;
        match parse_order(None) {
            OrderValue::Position(s) => assert_eq!(s, "last"),
            _ => panic!("Expected Position"),
        }
    }

    #[test]
    fn parse_order_last() {
        use roam_sdk::api::types::parse_order;
        match parse_order(Some("last")) {
            OrderValue::Position(s) => assert_eq!(s, "last"),
            _ => panic!("Expected Position"),
        }
    }

    #[test]
    fn parse_order_first() {
        use roam_sdk::api::types::parse_order;
        match parse_order(Some("first")) {
            OrderValue::Position(s) => assert_eq!(s, "first"),
            _ => panic!("Expected Position"),
        }
    }

    #[test]
    fn parse_order_numeric() {
        use roam_sdk::api::types::parse_order;
        match parse_order(Some("3")) {
            OrderValue::Index(n) => assert_eq!(n, 3),
            _ => panic!("Expected Index"),
        }
    }

    #[test]
    fn parse_order_invalid_defaults_to_last() {
        use roam_sdk::api::types::parse_order;
        match parse_order(Some("invalid")) {
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
                limit: None,
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
                limit: None,
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

    // -- search with limit --

    #[tokio::test]
    async fn search_with_limit_truncates_results() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": [
                    ["Page A", "a-uid"],
                    ["Page B", "b-uid"],
                    ["Page C", "c-uid"],
                    ["Page D", "d-uid"]
                ]
            })))
            .mount(&server)
            .await;

        let result = mcp
            .search(Parameters(SearchParams {
                query: "page".into(),
                limit: Some(2),
            }))
            .await;

        let result = result.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    // -- get_daily_note tool --

    #[tokio::test]
    async fn get_daily_note_with_specific_date() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    ":node/title": "March 15, 2026",
                    ":block/uid": "03-15-2026",
                    ":block/children": [{
                        ":block/uid": "b1",
                        ":block/string": "Today's note",
                        ":block/order": 0
                    }]
                }
            })))
            .mount(&server)
            .await;

        let result = mcp
            .get_daily_note(Parameters(GetDailyNoteParams {
                date: Some("2026-03-15".into()),
            }))
            .await;

        let result = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[":node/title"], "March 15, 2026");
    }

    #[tokio::test]
    async fn get_daily_note_today_calls_pull() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    ":node/title": "Today",
                    ":block/uid": "today-uid"
                }
            })))
            .mount(&server)
            .await;

        let result = mcp
            .get_daily_note(Parameters(GetDailyNoteParams { date: None }))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_daily_note_invalid_date_returns_error() {
        let (_server, mcp) = setup().await;

        let result = mcp
            .get_daily_note(Parameters(GetDailyNoteParams {
                date: Some("not-a-date".into()),
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid date format"));
    }

    // -- search_blocks tool --

    #[tokio::test]
    async fn search_blocks_filters_by_content() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": [
                    ["b1", "Hello world", "Page A"],
                    ["b2", "Goodbye world", "Page B"],
                    ["b3", "No match here", "Page C"]
                ]
            })))
            .mount(&server)
            .await;

        let result = mcp
            .search_blocks(Parameters(SearchBlocksParams {
                query: "world".into(),
                limit: None,
            }))
            .await;

        let result = result.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["uid"], "b1");
        assert_eq!(parsed[0]["page_title"], "Page A");
        assert_eq!(parsed[1]["uid"], "b2");
    }

    #[tokio::test]
    async fn search_blocks_respects_limit() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": [
                    ["b1", "match one", "P1"],
                    ["b2", "match two", "P2"],
                    ["b3", "match three", "P3"]
                ]
            })))
            .mount(&server)
            .await;

        let result = mcp
            .search_blocks(Parameters(SearchBlocksParams {
                query: "match".into(),
                limit: Some(2),
            }))
            .await;

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    // -- batch_write tool --

    #[tokio::test]
    async fn batch_write_sends_individual_requests() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(2) // each action is sent as a separate request
            .mount(&server)
            .await;

        let actions = json!([
            {"action": "create-page", "page": {"title": "Batch Page"}},
            {"action": "create-block", "location": {"parent-uid": "p1", "order": "last"}, "block": {"string": "Block 1"}}
        ]);

        let result = mcp
            .batch_write(Parameters(BatchWriteParams {
                actions: actions.to_string(),
            }))
            .await;

        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["actions"], 2);
        assert_eq!(parsed["status"], "executed");
    }

    #[tokio::test]
    async fn batch_write_invalid_json_returns_error() {
        let (_server, mcp) = setup().await;

        let result = mcp
            .batch_write(Parameters(BatchWriteParams {
                actions: "not json".into(),
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid actions JSON"));
    }

    // -- export_page_as_markdown tool --

    #[tokio::test]
    async fn export_page_as_markdown_formats_tree() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    ":node/title": "My Page",
                    ":block/uid": "page-uid",
                    ":block/children": [
                        {
                            ":block/uid": "b2",
                            ":block/string": "Second block",
                            ":block/order": 1
                        },
                        {
                            ":block/uid": "b1",
                            ":block/string": "First block",
                            ":block/order": 0,
                            ":block/children": [
                                {
                                    ":block/uid": "c1",
                                    ":block/string": "Child block",
                                    ":block/order": 0
                                }
                            ]
                        }
                    ]
                }
            })))
            .mount(&server)
            .await;

        let result = mcp
            .export_page_as_markdown(Parameters(ExportPageAsMarkdownParams {
                title: "My Page".into(),
            }))
            .await;

        let md = result.unwrap();
        assert!(md.starts_with("# My Page\n"));
        assert!(md.contains("- First block\n"));
        assert!(md.contains("  - Child block\n"));
        assert!(md.contains("- Second block\n"));
        let first_pos = md.find("First block").unwrap();
        let second_pos = md.find("Second block").unwrap();
        assert!(first_pos < second_pos);
    }

    // -- get_block_refs tool --

    #[tokio::test]
    async fn get_block_refs_returns_references() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    ":block/uid": "b1",
                    ":block/string": "Links to [[PageA]] and ((block-ref))",
                    ":block/refs": [
                        {":block/uid": "page-a-uid", ":node/title": "PageA"},
                        {":block/uid": "block-ref", ":block/string": "Referenced block"}
                    ]
                }
            })))
            .mount(&server)
            .await;

        let result = mcp
            .get_block_refs(Parameters(GetBlockRefsParams { uid: "b1".into() }))
            .await;

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["uid"], "page-a-uid");
        assert_eq!(parsed[0]["title"], "PageA");
        assert_eq!(parsed[1]["uid"], "block-ref");
        assert_eq!(parsed[1]["string"], "Referenced block");
    }

    #[tokio::test]
    async fn get_block_refs_empty_when_no_refs() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    ":block/uid": "b1",
                    ":block/string": "No links here"
                }
            })))
            .mount(&server)
            .await;

        let result = mcp
            .get_block_refs(Parameters(GetBlockRefsParams { uid: "b1".into() }))
            .await;

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed.is_empty());
    }

    // -- get_graph_stats tool --

    #[tokio::test]
    async fn get_graph_stats_returns_counts() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": [[42]]
            })))
            .mount(&server)
            .await;

        let result = mcp.get_graph_stats().await;

        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["pages"], 42);
        assert_eq!(parsed["blocks"], 42);
    }

    // -- create_block_with_children tool --

    #[tokio::test]
    async fn create_block_with_children_sends_batch() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .create_block_with_children(Parameters(CreateBlockWithChildrenParams {
                parent_uid: "parent-uid".into(),
                content: "Parent block".into(),
                order: Some("last".into()),
                uid: Some("custom-uid".into()),
                children: Some(r#"["Child 1", "Child 2"]"#.into()),
            }))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("custom-uid"));
    }

    #[tokio::test]
    async fn create_block_with_children_no_children_single_write() {
        let (server, mcp) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = mcp
            .create_block_with_children(Parameters(CreateBlockWithChildrenParams {
                parent_uid: "parent-uid".into(),
                content: "Solo block".into(),
                order: None,
                uid: None,
                children: None,
            }))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn create_block_with_children_invalid_json_returns_error() {
        let (_server, mcp) = setup().await;

        let result = mcp
            .create_block_with_children(Parameters(CreateBlockWithChildrenParams {
                parent_uid: "parent-uid".into(),
                content: "Block".into(),
                order: None,
                uid: None,
                children: Some("not json".into()),
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid children JSON"));
    }

    // -- render_block_as_markdown helper --

    #[test]
    fn render_block_as_markdown_nested() {
        let block = json!({
            ":block/string": "Parent",
            ":block/order": 0,
            ":block/children": [
                {
                    ":block/string": "Child B",
                    ":block/order": 1
                },
                {
                    ":block/string": "Child A",
                    ":block/order": 0,
                    ":block/children": [
                        {
                            ":block/string": "Grandchild",
                            ":block/order": 0
                        }
                    ]
                }
            ]
        });

        let mut output = String::new();
        render_block_as_markdown(&block, 0, &mut output);
        assert_eq!(
            output,
            "- Parent\n  - Child A\n    - Grandchild\n  - Child B\n"
        );
    }

    // -- WriteAction batch serialization --

    #[test]
    fn batch_actions_serializes_correctly() {
        let batch = WriteAction::BatchActions {
            actions: vec![
                WriteAction::CreatePage {
                    page: PageCreate {
                        title: "Test".into(),
                        uid: None,
                    },
                },
                WriteAction::UpdateBlock {
                    block: BlockUpdate {
                        uid: "b1".into(),
                        string: "Updated".into(),
                    },
                },
            ],
        };
        let json = serde_json::to_value(&batch).unwrap();
        assert_eq!(json["action"], "batch-actions");
        assert_eq!(json["actions"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn batch_actions_deserializes() {
        let json = json!([
            {"action": "create-page", "page": {"title": "New"}},
            {"action": "delete-block", "block": {"uid": "x"}}
        ]);
        let actions: Vec<WriteAction> = serde_json::from_value(json).unwrap();
        assert_eq!(actions.len(), 2);
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
