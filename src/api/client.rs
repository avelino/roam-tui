use reqwest::Client;

use crate::api::types::{PullRequest, PullResponse, QueryRequest, QueryResponse, WriteAction};
use crate::error::{Result, RoamError};

#[derive(Clone)]
pub struct RoamClient {
    client: Client,
    base_url: String,
    token: String,
}

impl RoamClient {
    pub fn new(graph_name: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("https://api.roamresearch.com/api/graph/{}", graph_name),
            token: token.to_string(),
        }
    }

    #[cfg(test)]
    pub fn new_with_base_url(base_url: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            token: token.to_string(),
        }
    }

    pub async fn pull(&self, eid: serde_json::Value, selector: &str) -> Result<PullResponse> {
        let req = PullRequest {
            eid,
            selector: selector.to_string(),
        };
        let resp = self
            .client
            .post(format!("{}/pull", self.base_url))
            .header("X-Authorization", format!("Bearer {}", self.token))
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(RoamError::Api { status, message });
        }

        let body = resp.json::<PullResponse>().await?;
        Ok(body)
    }

    pub async fn query(
        &self,
        query: String,
        args: Vec<serde_json::Value>,
    ) -> Result<QueryResponse> {
        let req = QueryRequest { query, args };
        let resp = self
            .client
            .post(format!("{}/q", self.base_url))
            .header("X-Authorization", format!("Bearer {}", self.token))
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(RoamError::Api { status, message });
        }

        let body = resp.json::<QueryResponse>().await?;
        Ok(body)
    }

    pub async fn write(&self, action: WriteAction) -> Result<()> {
        let resp = self
            .client
            .post(format!("{}/write", self.base_url))
            .header("X-Authorization", format!("Bearer {}", self.token))
            .json(&action)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(RoamError::Api { status, message });
        }

        Ok(())
    }
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

    #[tokio::test]
    async fn pull_sends_correct_request() {
        let (server, client) = setup().await;

        Mock::given(method("POST"))
            .and(path("/pull"))
            .and(header("X-Authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"result": {":block/uid": "abc", ":block/string": "hello"}}),
                ),
            )
            .mount(&server)
            .await;

        let resp = client
            .pull(json!(["block/uid", "abc"]), "[:block/string :block/uid]")
            .await
            .unwrap();

        assert_eq!(resp.result[":block/uid"], "abc");
    }

    #[tokio::test]
    async fn write_sends_correct_request() {
        let (server, client) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .and(header("X-Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let result = client
            .write(WriteAction::UpdateBlock {
                block: crate::api::types::BlockUpdate {
                    uid: "abc".into(),
                    string: "Updated".into(),
                },
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn write_returns_error_on_500() {
        let (server, client) = setup().await;

        Mock::given(method("POST"))
            .and(path("/write"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let err = client
            .write(WriteAction::DeleteBlock {
                block: crate::api::types::BlockRef { uid: "abc".into() },
            })
            .await;

        assert!(err.is_err());
        match err.unwrap_err() {
            RoamError::Api { status, .. } => assert_eq!(status, 500),
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn query_sends_correct_request() {
        let (server, client) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .and(header("X-Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "result": [[{":block/string": "ref text", ":block/uid": "abc"}]]
            })))
            .mount(&server)
            .await;

        let resp = client
            .query("[:find ?b :where [?b :block/string]]".into(), vec![])
            .await
            .unwrap();

        assert_eq!(resp.result.len(), 1);
    }

    #[tokio::test]
    async fn query_returns_error_on_500() {
        let (server, client) = setup().await;

        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let err = client
            .query("[:find ?b :where [?b :block/string]]".into(), vec![])
            .await;

        assert!(err.is_err());
        match err.unwrap_err() {
            RoamError::Api { status, .. } => assert_eq!(status, 500),
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }
}
