use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::collections::HashMap;

pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }

    pub async fn get(
        &self,
        url: String,
        headers: Option<HashMap<String, String>>,
        parse_json: bool,
    ) -> Result<Value, String> {
        let mut request = self.client.get(&url);

        if let Some(headers) = headers {
            let mut header_map = HeaderMap::new();
            for (k, v) in headers {
                header_map.insert(
                    k.parse::<HeaderName>().map_err(|e| e.to_string())?,
                    v.parse::<HeaderValue>().map_err(|e| e.to_string())?,
                );
            }
            request = request.headers(header_map);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;

        if parse_json {
            response.json::<Value>().await.map_err(|e| e.to_string())
        } else {
            Ok(Value::String(response.text().await.map_err(|e| e.to_string())?))
        }
    }

    pub async fn post(
        &self,
        url: String,
        body: String,
        headers: Option<HashMap<String, String>>,
        parse_json: bool,
    ) -> Result<Value, String> {
        let mut request = self.client.post(&url).body(body);

        if let Some(headers) = headers {
            let mut header_map = HeaderMap::new();
            for (k, v) in headers {
                header_map.insert(
                    k.parse::<HeaderName>().map_err(|e| e.to_string())?,
                    v.parse::<HeaderValue>().map_err(|e| e.to_string())?,
                );
            }
            request = request.headers(header_map);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;

        if parse_json {
            response.json::<Value>().await.map_err(|e| e.to_string())
        } else {
            Ok(Value::String(response.text().await.map_err(|e| e.to_string())?))
        }
    }
}
