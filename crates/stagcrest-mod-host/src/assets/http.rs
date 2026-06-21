use super::AssetError;
use gloo_net::http::Request;

pub struct HttpAssetReader;

impl HttpAssetReader {
    pub fn new() -> Self {
        Self
    }

    fn url(path: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("/{path}")
    }

    pub async fn read_bytes_async(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let url = Self::url(path);
        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| AssetError::Http(e.to_string()))?;
        if !response.ok() {
            return Err(AssetError::NotFound(path.to_string()));
        }
        response
            .binary()
            .await
            .map_err(|e| AssetError::Http(e.to_string()))
    }

    pub async fn exists_async(&self, path: &str) -> bool {
        let url = Self::url(path);
        match Request::get(&url).send().await {
            Ok(resp) => resp.ok(),
            Err(_) => false,
        }
    }
}

impl Default for HttpAssetReader {
    fn default() -> Self {
        Self::new()
    }
}
