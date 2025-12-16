//! PuppetDB client service

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::config::PuppetDbConfig;
use crate::models::{Fact, Node, Report};

/// PuppetDB API client
#[derive(Clone)]
pub struct PuppetDbClient {
    client: Client,
    base_url: String,
}

impl PuppetDbClient {
    /// Create a new PuppetDB client
    pub fn new(config: &PuppetDbConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: config.url.trim_end_matches('/').to_string(),
        })
    }

    /// Execute a PQL query
    pub async fn query<T: DeserializeOwned>(&self, query: &str) -> Result<Vec<T>> {
        let url = format!("{}/pdb/query/v4", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&PqlQuery { query: query.to_string() })
            .send()
            .await
            .context("Failed to send PQL query")?;

        let results = response
            .json::<Vec<T>>()
            .await
            .context("Failed to parse PQL response")?;

        Ok(results)
    }

    /// Get all nodes
    pub async fn get_nodes(&self) -> Result<Vec<Node>> {
        let url = format!("{}/pdb/query/v4/nodes", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch nodes")?;

        let nodes = response
            .json::<Vec<Node>>()
            .await
            .context("Failed to parse nodes response")?;

        Ok(nodes)
    }

    /// Get a specific node by certname
    pub async fn get_node(&self, certname: &str) -> Result<Option<Node>> {
        let url = format!("{}/pdb/query/v4/nodes/{}", self.base_url, certname);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch node")?;

        if response.status().is_success() {
            let node = response
                .json::<Node>()
                .await
                .context("Failed to parse node response")?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    /// Get facts for a node
    pub async fn get_node_facts(&self, certname: &str) -> Result<Vec<Fact>> {
        let url = format!(
            "{}/pdb/query/v4/nodes/{}/facts",
            self.base_url, certname
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch node facts")?;

        let facts = response
            .json::<Vec<Fact>>()
            .await
            .context("Failed to parse facts response")?;

        Ok(facts)
    }

    /// Get reports for a node
    pub async fn get_node_reports(&self, certname: &str, limit: Option<u32>) -> Result<Vec<Report>> {
        let mut url = format!(
            "{}/pdb/query/v4/nodes/{}/reports",
            self.base_url, certname
        );

        if let Some(limit) = limit {
            url.push_str(&format!("?limit={}", limit));
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch node reports")?;

        let reports = response
            .json::<Vec<Report>>()
            .await
            .context("Failed to parse reports response")?;

        Ok(reports)
    }

    /// Query facts across all nodes
    pub async fn query_facts(&self, fact_name: Option<&str>) -> Result<Vec<Fact>> {
        let mut url = format!("{}/pdb/query/v4/facts", self.base_url);

        if let Some(name) = fact_name {
            url.push_str(&format!("?query=[\"=\",\"name\",\"{}\"]", name));
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch facts")?;

        let facts = response
            .json::<Vec<Fact>>()
            .await
            .context("Failed to parse facts response")?;

        Ok(facts)
    }

    /// Get all unique fact names
    pub async fn get_fact_names(&self) -> Result<Vec<String>> {
        let url = format!("{}/pdb/query/v4/fact-names", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch fact names")?;

        let names = response
            .json::<Vec<String>>()
            .await
            .context("Failed to parse fact names response")?;

        Ok(names)
    }

    /// Query reports
    pub async fn query_reports(
        &self,
        certname: Option<&str>,
        status: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<Report>> {
        let mut query_parts: Vec<String> = vec![];

        if let Some(cn) = certname {
            query_parts.push(format!("[\"=\",\"certname\",\"{}\"]", cn));
        }

        if let Some(st) = status {
            query_parts.push(format!("[\"=\",\"status\",\"{}\"]", st));
        }

        let mut url = format!("{}/pdb/query/v4/reports", self.base_url);
        let mut params: Vec<String> = vec![];

        if !query_parts.is_empty() {
            let query = if query_parts.len() == 1 {
                query_parts[0].clone()
            } else {
                format!("[\"and\",{}]", query_parts.join(","))
            };
            params.push(format!("query={}", query));
        }

        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }

        if !params.is_empty() {
            url.push_str(&format!("?{}", params.join("&")));
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch reports")?;

        let reports = response
            .json::<Vec<Report>>()
            .await
            .context("Failed to parse reports response")?;

        Ok(reports)
    }
}

#[derive(Serialize)]
struct PqlQuery {
    query: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_url_construction() {
        let config = PuppetDbConfig {
            url: "http://localhost:8081/".to_string(),
            timeout_secs: 30,
            ssl_verify: true,
            ssl_cert: None,
            ssl_key: None,
            ssl_ca: None,
        };

        let client = PuppetDbClient::new(&config).unwrap();
        assert_eq!(client.base_url, "http://localhost:8081");
    }
}
