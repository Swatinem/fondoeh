use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct Cacher {
    inner: Arc<CacheInner>,
}

#[derive(Debug)]
struct CacheInner {
    client: reqwest::Client,
    cache_dir: PathBuf,
}

impl Cacher {
    pub async fn new() -> Result<Self> {
        let client = reqwest::Client::new();
        let cache_dir = ".cache".into();
        fs::create_dir_all(&cache_dir).await?;
        let inner = Arc::new(CacheInner { client, cache_dir });
        Ok(Self { inner })
    }

    pub fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.inner.client.get(url)
    }

    #[tracing::instrument(skip_all, fields(url))]
    pub async fn get_request(&self, key: &str, builder: reqwest::RequestBuilder) -> Result<String> {
        let (client, request) = builder.build_split();
        let request = request?;
        tracing::Span::current().record("url", request.url().to_string());

        let path = self.inner.cache_dir.join(format!("{key}.txt"));
        if let Ok(inhalt) = fs::read_to_string(&path).await {
            return Ok(inhalt);
        }

        let response = client.execute(request).await?;
        let inhalt = response.text().await?;

        if let Err(err) = fs::write(path, &inhalt).await {
            tracing::error!(err = &err as &dyn std::error::Error);
        }

        Ok(inhalt)
    }
}
