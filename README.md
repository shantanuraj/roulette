# roulette

`roulette` is a small HTTP service that returns a random image by issuing an
HTTP redirect to an externally hosted asset.

## Motivation

Serving a random photo for a personal website from a large catalog that would be
impractical to embed in a static site, and would require JavaScript on the
client side to select.

Not limited to images; any static asset type works where random selection is
desired without exposing the full index.

## Architecture

```
Client → GET /image → roulette → 302 redirect → Asset host (S3, CDN, etc.)
```

## Configuration

| Variable                  | Required | Description                                            |
| ------------------------- | -------- | ------------------------------------------------------ |
| `IMAGE_URL_PREFIX`        | yes      | Base URL for image filenames                           |
| `IMAGE_MAP_PATH`          | no       | Path to JSON map (default: embedded)                   |
| `IMAGE_MAP_SYNC_URL`      | no       | URL to fetch updated map from                          |
| `IMAGE_MAP_SYNC_INTERVAL` | no       | Sync interval in seconds                               |
| `RECENCY_DECAY`           | no       | Exponential decay rate for `/latest` (default: `0.05`) |
| `PORT`                    | no       | HTTP port (default: `8080`)                            |
| `RUST_LOG`                | no       | Log level (e.g. `info`, `tower_http=debug`)            |

## Image Map

```json
{
  "2024-01-09_00-07-20_UTC.jpg": "8c1923e1-768c-43a0-9963-6909cdd8a442.jpg"
}
```

Keys are timestamp-prefixed identifiers; values are filenames on the asset host.

Redirect URL: `{IMAGE_URL_PREFIX}/{value}`

The map is embedded at compile time. Set `IMAGE_MAP_PATH` to override, or
configure sync for hot reload.

## API

### `GET /health`

Reports total number of loaded images in the body.

### `GET /image`

Uniform random selection from all images.

### `GET /image/after/{bound}`

Uniform random from images with keys `>= bound`.

```
/image/after/2024
/image/after/2024-06
/image/after/2024-06-15
```

### `GET /image/latest`

Recency-biased random selection (exponential weighting toward newer images).

### `GET /image/latest/after/{bound}`

Recency-biased selection from filtered set.

### Cache Control

All image endpoints accept `?cache={duration}` to set `Cache-Control: public, max-age={seconds}`.

| Format | Example |
| ------ | ------- |
| `{n}s` | `60s`   |
| `{n}m` | `5m`    |
| `{n}h` | `1h`    |
| `{n}d` | `7d`    |

```
/image?cache=1h
/image/latest/after/2024?cache=5m
```

## Runtime

- Rust + axum + Tokio
- In-memory map behind RwLock
- O(1) request handling
- Graceful shutdown on SIGTERM/SIGINT
- Optional hot reload via sync
