# roulette

`roulette` is a small HTTP service that returns a random image by issuing an
HTTP redirect to an externally hosted asset.

The service exists to avoid publishing the full image index to clients while
keeping image delivery off the application server.

## Motivation

My primary use case is serving a random photo for a personal website, reading
from a large catalog that would be impractical to embed in a static site;
and would still require JavaScript on the client side to select an image.

In theory, we are not limited to images; any static asset type could be served
where random selection is desired without exposing the full index.
The asset doesn't even have to be static, as long as the URL resolves to a valid
resource and can respond to a `GET` request.

## Architecture Overview

```
Client
└── GET /image
└── roulette (axum)
├── load image-map.json (startup)
├── select random entry
├── resolve full image URL
└── HTTP redirect (302/307)
└── Asset host (S3, CDN, etc.)
```

## Image Map

Images are defined in a JSON file (image-map.json) loaded at startup.

```json
{
  "2020-01-09_00-07-20_UTC.jpg": "8c1923e1-768c-43a0-9963-6909cdd8a442.jpg"
}
```

- Key: original or logical image identifier (opaque to the service)

- Value: resolved filename used by the asset host

Only the values are used to construct the final URL.
Keys are retained for traceability and potential future metadata use.

The map is loaded at startup and kept in memory.
Optional background sync can reload the map from a remote URL without restart.

## URL Resolution

The final redirect URL is constructed as:

```
{IMAGE_URL_PREFIX}/{mapped_filename}
```

Example:

```
IMAGE_URL_PREFIX=https://s3.eu-north-1.amazonaws.com/example.com/images
```

Resulting redirect:

```
https://s3.eu-north-1.amazonaws.com/example.com/images/8c1923e1-768c-43a0-9963-6909cdd8a442.jpg
```

The service is intentionally storage-agnostic. Any object store or CDN can be
used as long as it serves static assets over HTTP(S).

## HTTP API

### GET `/health`

Returns `200 OK` for health checks and load balancer probes.
Reports total number of loaded images in the body.

### GET `/image`

Returns a redirect response to a randomly selected image.

Redirect status: `302 Found` or `307 Temporary Redirect`

Response body: empty

Image selection is uniform across the loaded map

## Cache Control

All image endpoints accept an optional `cache` query parameter to control
client and CDN caching of the redirect response.

```
GET /image?cache=1h
GET /image/after/2024?cache=5m
GET /image/latest?cache=30s
GET /image/latest/after/2024?cache=1d
```

### Supported duration formats

| Suffix | Unit    | Example |
|--------|---------|---------|
| `s`    | seconds | `60s`   |
| `m`    | minutes | `5m`    |
| `h`    | hours   | `1h`    |
| `d`    | days    | `7d`    |

When provided, the response includes `Cache-Control: public, max-age={seconds}`.
When omitted, no cache header is set.

## Key-Based Filtering & Selection Semantics

`roulette` treats the **keys** in `image-map.json` as structured identifiers
rather than opaque strings.

By convention, keys encode a timestamp prefix:

```
YYYY-MM-DD_HH-MM-SS_UTC*.jpg
```

This allows the service to apply selection constraints without additional
metadata or schema changes.

## Time-Based Eligibility (`after` constraint)

The service supports restricting the eligible image set based on a lower time
bound derived from the key.

### Endpoint

```
GET /image/after/{year}
GET /image/after/{year-month}
GET /image/after/{year-month-day}
```

Examples:

```
/image/after/2023
/image/after/2023-06
/image/after/2023-06-01
```

### Semantics

- The `{after}` segment defines an **inclusive lower bound**
- Images whose keys sort lexicographically _before_ the bound are excluded
- Remaining images participate in random selection

If the filtered set is empty, the service returns a `404`.

## Random Selection Modes

### Uniform Random (default)

All eligible images have equal probability of being selected.

Used by:

```
GET /image
GET /image/after/{...}
```

### Recency-Biased Random

An additional endpoint introduces bias toward newer images while preserving
randomness and respecting eligibility constraints.

### Endpoint

```
GET /image/latest
GET /image/latest/after/{...}
```

### Semantics

- The candidate set is first filtered using the same `after` rules
- Selection probability increases with recency
- Older images remain selectable, but with reduced likelihood

The exact weighting strategy is an implementation detail (e.g. exponential
decay, linear weighting, or bucketed ranges) and may change without affecting
the API contract.

The goal is:

- Prefer newer images
- Avoid determinism
- Avoid starvation of older but still valid images

## Notes

- Key format is assumed to be sortable lexicographically by time
- Behavior is undefined for keys that do not follow the timestamp prefix
  convention
- Future extensions may add:
  - upper bounds (`before`)
  - explicit weighting policies
  - non-time-based key semantics

### Configuration

| Environment Variable      | Required | Description                                                  |
| ------------------------- | -------- | ------------------------------------------------------------ |
| `IMAGE_URL_PREFIX`        | yes      | Base URL used to resolve image filenames                     |
| `IMAGE_MAP_PATH`          | no       | Path to `image-map.json` (default: embedded at compile time) |
| `IMAGE_MAP_SYNC_URL`      | no       | URL to fetch updated image map from                          |
| `IMAGE_MAP_SYNC_INTERVAL` | no       | Sync interval in seconds (requires `IMAGE_MAP_SYNC_URL`)     |
| `PORT`                    | no       | Port to bind the HTTP server (default: `8080`)               |
| `RUST_LOG`                | no       | Log level filter (e.g. `info`, `debug`, `tower_http=debug`)  |

## Framework & Runtime

Language: Rust

Web framework: axum

Concurrency model: async (Tokio)

State: in-memory image map behind RwLock, hot-reloadable via sync

The request path performs no allocations beyond RNG and header construction.

## Design Notes

- No client-visible index
- No image data handled by the service
- O(1) request handling
- Suitable for serverless or containerized deployment
- Health endpoint at `/health` for load balancer probes
- Graceful shutdown on SIGTERM/SIGINT
- Optional hot reload via `IMAGE_MAP_SYNC_URL`
- Easy to extend with:
  - rate limiting
  - weighted randomness
  - multiple endpoints

### Non-goals (for now)

- Image validation
- Auth
- Persistence beyond startup
- Request-based determinism
