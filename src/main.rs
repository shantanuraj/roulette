use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rand::{distributions::WeightedIndex, prelude::*};
use std::{collections::HashMap, env, fs, sync::Arc};

const EMBEDDED_IMAGE_MAP: &str = include_str!("../image-map.json");

struct AppState {
    url_prefix: String,
    sorted_keys: Vec<String>,
    map: HashMap<String, String>,
}

impl AppState {
    fn load() -> Self {
        let url_prefix = env::var("IMAGE_URL_PREFIX").expect("IMAGE_URL_PREFIX required");
        let content = env::var("IMAGE_MAP_PATH")
            .map(|p| fs::read_to_string(p).expect("failed to read image map"))
            .unwrap_or_else(|_| EMBEDDED_IMAGE_MAP.to_string());
        let map: HashMap<String, String> = serde_json::from_str(&content).expect("invalid JSON");
        let mut sorted_keys: Vec<String> = map.keys().cloned().collect();
        sorted_keys.sort();
        Self { url_prefix, sorted_keys, map }
    }

    fn select_uniform<'a>(&self, keys: &'a [String]) -> Option<&'a str> {
        if keys.is_empty() {
            return None;
        }
        let idx = thread_rng().gen_range(0..keys.len());
        Some(&keys[idx])
    }

    fn select_biased<'a>(&self, keys: &'a [String]) -> Option<&'a str> {
        if keys.is_empty() {
            return None;
        }
        let decay = 0.05;
        let weights: Vec<f64> = (0..keys.len()).map(|i| (i as f64 * decay).exp()).collect();
        let dist = WeightedIndex::new(&weights).ok()?;
        Some(&keys[thread_rng().sample(dist)])
    }

    fn filter_after(&self, bound: &str) -> &[String] {
        let start = self.sorted_keys.partition_point(|k| k.as_str() < bound);
        &self.sorted_keys[start..]
    }

    fn redirect(&self, key: &str) -> Response {
        let filename = &self.map[key];
        let url = format!("{}/{}", self.url_prefix, filename);
        (StatusCode::FOUND, [(header::LOCATION, url)]).into_response()
    }
}

async fn random_image(State(state): State<Arc<AppState>>) -> Response {
    match state.select_uniform(&state.sorted_keys) {
        Some(key) => state.redirect(key),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn random_image_after(State(state): State<Arc<AppState>>, Path(bound): Path<String>) -> Response {
    let keys = state.filter_after(&bound);
    match state.select_uniform(keys) {
        Some(key) => state.redirect(key),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn latest_image(State(state): State<Arc<AppState>>) -> Response {
    match state.select_biased(&state.sorted_keys) {
        Some(key) => state.redirect(key),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn latest_image_after(State(state): State<Arc<AppState>>, Path(bound): Path<String>) -> Response {
    let keys = state.filter_after(&bound);
    match state.select_biased(keys) {
        Some(key) => state.redirect(key),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let state = Arc::new(AppState::load());
    let app = Router::new()
        .route("/image", get(random_image))
        .route("/image/after/{bound}", get(random_image_after))
        .route("/image/latest", get(latest_image))
        .route("/image/latest/after/{bound}", get(latest_image_after))
        .with_state(state);
    let port: u16 = env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
