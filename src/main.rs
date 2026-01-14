use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rand::{distributions::WeightedIndex, prelude::*};
use serde::Deserialize;
use std::{
    collections::HashMap,
    env, fs,
    hash::{Hash, Hasher},
    sync::Arc,
    sync::RwLock,
    time::Duration,
};
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

#[derive(Deserialize, Default)]
struct CacheQuery {
    cache: Option<String>,
}

fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, suffix) = s.split_at(s.len().saturating_sub(1));
    let value: u64 = num.parse().ok()?;
    match suffix {
        "s" => Some(value),
        "m" => Some(value * 60),
        "h" => Some(value * 3600),
        "d" => Some(value * 86400),
        _ => None,
    }
}

const EMBEDDED_IMAGE_MAP: &str = include_str!("../image-map.json");

fn hash_content(content: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

struct ImageMap {
    sorted_keys: Vec<String>,
    map: HashMap<String, String>,
    content_hash: u64,
}

impl ImageMap {
    fn parse(content: &str) -> Result<Self, serde_json::Error> {
        let map: HashMap<String, String> = serde_json::from_str(content)?;
        let mut sorted_keys: Vec<String> = map.keys().cloned().collect();
        sorted_keys.sort();
        Ok(Self {
            sorted_keys,
            map,
            content_hash: hash_content(content),
        })
    }
}

struct AppState {
    url_prefix: String,
    image_map: RwLock<ImageMap>,
}

impl AppState {
    fn load() -> Self {
        let url_prefix = env::var("IMAGE_URL_PREFIX").expect("IMAGE_URL_PREFIX required");
        let content = env::var("IMAGE_MAP_PATH")
            .map(|p| fs::read_to_string(p).expect("failed to read image map"))
            .unwrap_or_else(|_| EMBEDDED_IMAGE_MAP.to_string());
        let image_map = ImageMap::parse(&content).expect("invalid JSON");
        Self {
            url_prefix,
            image_map: RwLock::new(image_map),
        }
    }

    fn redirect(
        &self,
        key: &str,
        map: &HashMap<String, String>,
        cache_secs: Option<u64>,
    ) -> Response {
        let url = format!("{}/{}", self.url_prefix, map[key]);
        match cache_secs {
            Some(secs) => (
                StatusCode::FOUND,
                [
                    (header::LOCATION, url),
                    (header::CACHE_CONTROL, format!("public, max-age={}", secs)),
                ],
            )
                .into_response(),
            None => (StatusCode::FOUND, [(header::LOCATION, url)]).into_response(),
        }
    }
}

fn select_uniform(keys: &[String]) -> Option<&str> {
    if keys.is_empty() {
        return None;
    }
    Some(&keys[thread_rng().gen_range(0..keys.len())])
}

fn select_biased(keys: &[String]) -> Option<&str> {
    if keys.is_empty() {
        return None;
    }
    let decay = 0.05;
    let weights: Vec<f64> = (0..keys.len()).map(|i| (i as f64 * decay).exp()).collect();
    let dist = WeightedIndex::new(&weights).ok()?;
    Some(&keys[thread_rng().sample(dist)])
}

fn filter_after<'a>(keys: &'a [String], bound: &str) -> &'a [String] {
    let start = keys.partition_point(|k| k.as_str() < bound);
    &keys[start..]
}

fn maybe_parse_if_changed(content: &str, current_hash: u64) -> Option<ImageMap> {
    if hash_content(content) == current_hash {
        return None;
    }
    ImageMap::parse(content).ok()
}

async fn random_image(State(state): State<Arc<AppState>>, Query(q): Query<CacheQuery>) -> Response {
    let cache = q.cache.as_deref().and_then(parse_duration);
    let guard = state.image_map.read().unwrap();
    match select_uniform(&guard.sorted_keys) {
        Some(key) => state.redirect(key, &guard.map, cache),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn random_image_after(
    State(state): State<Arc<AppState>>,
    Path(bound): Path<String>,
    Query(q): Query<CacheQuery>,
) -> Response {
    let cache = q.cache.as_deref().and_then(parse_duration);
    let guard = state.image_map.read().unwrap();
    let keys = filter_after(&guard.sorted_keys, &bound);
    match select_uniform(keys) {
        Some(key) => state.redirect(key, &guard.map, cache),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn latest_image(State(state): State<Arc<AppState>>, Query(q): Query<CacheQuery>) -> Response {
    let cache = q.cache.as_deref().and_then(parse_duration);
    let guard = state.image_map.read().unwrap();
    match select_biased(&guard.sorted_keys) {
        Some(key) => state.redirect(key, &guard.map, cache),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn latest_image_after(
    State(state): State<Arc<AppState>>,
    Path(bound): Path<String>,
    Query(q): Query<CacheQuery>,
) -> Response {
    let cache = q.cache.as_deref().and_then(parse_duration);
    let guard = state.image_map.read().unwrap();
    let keys = filter_after(&guard.sorted_keys, &bound);
    match select_biased(keys) {
        Some(key) => state.redirect(key, &guard.map, cache),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn health(State(state): State<Arc<AppState>>) -> String {
    state
        .image_map
        .read()
        .unwrap()
        .sorted_keys
        .len()
        .to_string()
}

async fn robots() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain")],
        "User-agent: *\nDisallow: /\n",
    )
}

async fn sync_loop(state: Arc<AppState>, url: String, interval: Duration) {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static("roulette/1.0"),
    );
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .expect("failed to build HTTP client");
    loop {
        match client
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => match resp.text().await {
                Ok(content) => {
                    let current_hash = state.image_map.read().unwrap().content_hash;
                    if let Some(new_map) = maybe_parse_if_changed(&content, current_hash) {
                        info!(images = new_map.sorted_keys.len(), "synced image map");
                        *state.image_map.write().unwrap() = new_map;
                    }
                }
                Err(e) => warn!(error = %e, "sync failed: read error"),
            },
            Err(e) => warn!(error = %e, "sync failed: fetch error"),
        }
        tokio::time::sleep(interval).await;
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown signal received");
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let state = Arc::new(AppState::load());
    info!(
        images = state.image_map.read().unwrap().sorted_keys.len(),
        "loaded image map"
    );
    if let (Ok(url), Ok(secs)) = (
        env::var("IMAGE_MAP_SYNC_URL"),
        env::var("IMAGE_MAP_SYNC_INTERVAL"),
    ) {
        let interval = Duration::from_secs(
            secs.parse()
                .expect("IMAGE_MAP_SYNC_INTERVAL must be seconds"),
        );
        info!(%url, ?interval, "starting sync loop");
        tokio::spawn(sync_loop(state.clone(), url, interval));
    }
    let app = Router::new()
        .route("/health", get(health))
        .route("/image", get(random_image))
        .route("/image/after/{bound}", get(random_image_after))
        .route("/image/latest", get(latest_image))
        .route("/image/latest/after/{bound}", get(latest_image_after))
        .route("/robots.txt", get(robots))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    info!(port, "starting server");
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

#[cfg(test)]
mod tests;
