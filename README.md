# Sidekick

> **⚠ Beta** — Sidekick is under active development. APIs may change between releases. Not recommended for production use without thorough testing.

**Sub-microsecond feature flag evaluation. No network calls. No polling. Self-hosted.**

Sidekick is a high-performance feature flag and targeting engine built in Rust. Flags are evaluated **in-process** inside each SDK — completely off the network critical path — while a persistent SSE stream keeps every client synchronized in real time.

---

## Table of Contents

- [Why Sidekick](#why-sidekick)
- [How It Compares](#how-it-compares)
- [Architecture](#architecture)
- [Core Concepts](#core-concepts)
- [Getting Started](#getting-started)
- [Environment Variables](#environment-variables)
- [API Reference](#api-reference)
- [SDK Usage](#sdk-usage)
  - [Node.js](#nodejs)
  - [Browser (WebAssembly)](#browser-webassembly)
  - [React Native (JSI)](#react-native-jsi)
  - [Flutter (FFI)](#flutter-ffi)
- [Deployment](#deployment)
  - [Docker](#docker)
  - [AWS](#aws)
  - [CI/CD](#cicd)
- [Development](#development)
- [Repository Structure](#repository-structure)
- [Dependencies](#dependencies)
- [License](#license)

---

## Why Sidekick

Most feature flag systems make a **network call** every time you check a flag — or they poll a remote server on an interval and accept stale data in between. Both approaches add latency to every guarded code path, create availability dependencies, and don't scale to high-frequency evaluations (e.g. per-request, per-render, per-frame).

Sidekick takes a fundamentally different approach:

| Property | Sidekick | Polling-based systems | Request-time network systems |
|---|---|---|---|
| Evaluation latency | **< 1 µs** (in-process) | 1–50 ms (cached) | 10–300 ms (network) |
| Network dependency at eval | **None** | None | **Hard dependency** |
| Staleness window | **~0 ms** (SSE push) | Poll interval (30–60 s) | None (always fresh, but slow) |
| Works offline / on bad network | **Yes** (last-known state) | Yes (until TTL) | **No** |
| Evaluation cost scales with RPS | **No** (in-memory) | No | **Yes** (network/compute) |

### The key insight

The server is only involved in **propagation**, not in **evaluation**. Flag changes travel from the control plane → Redis pub/sub → SSE → SDK in-memory cache. After that, `isEnabled()` is a pure in-process lookup — no serialization, no I/O, no allocations.

---

## How It Compares

### vs. LaunchDarkly

LaunchDarkly's SDKs also cache flags locally, but:

- **Cost**: LaunchDarkly starts at $8.33/seat/month and scales steeply. Sidekick is self-hosted — your only cost is the compute you already run.
- **Data residency**: LaunchDarkly processes flag evaluations on their infrastructure. Sidekick never sends user data off your servers — evaluation is entirely local.
- **Vendor lock-in**: Migrating away from LaunchDarkly requires re-implementing targeting logic. Sidekick is open, self-hosted, and the evaluation engine is open source.
- **SDK performance**: LaunchDarkly's JS SDK evaluates in JavaScript. Sidekick's browser SDK evaluates inside WebAssembly compiled from Rust — lower and more predictable latency.
- **React Native**: LaunchDarkly's React Native SDK polls via HTTP. Sidekick uses JSI — a synchronous C++ bridge — so `isEnabled()` never crosses the JS↔native async boundary.

### vs. Unleash (open source)

Unleash is a strong open source option but:

- **Evaluation is server-side by default** in many Unleash configurations. The "local evaluation" mode requires the Enterprise plan for some SDK types.
- **Mobile SDKs poll**: Unleash mobile SDKs typically poll the `/api/client/features` endpoint on a configurable interval (default 15 s). Sidekick uses a persistent SSE stream — updates arrive within milliseconds.
- **No WASM/JSI**: Unleash has no WebAssembly browser SDK or JSI React Native SDK. Evaluation in these environments happens in JavaScript.
- **Rust evaluation**: Sidekick's evaluation engine is Rust compiled into each SDK binary. Hash distribution, targeting rule matching, and flag storage are all native-speed.

### vs. Flagsmith (open source)

- **Server-side evaluation**: Flagsmith's free tier evaluates flags server-side (network round-trip per evaluation). Local evaluation requires a paid plan.
- **No SSE**: Flagsmith uses REST polling, not push. The minimum poll interval is 60 seconds.
- **No native mobile evaluation**: Flagsmith's React Native and Flutter SDKs call the API for every evaluation or cache locally via REST polling. Sidekick ships a native Rust binary into your app.

### vs. OpenFeature (standard)

OpenFeature is a vendor-neutral SDK specification, not an implementation. You still need a provider (LaunchDarkly, Unleash, Flagsmith, etc.) with all their associated trade-offs. Sidekick is a complete, self-contained implementation — you don't need a separate provider.

### vs. Growthbook (open source)

Growthbook focuses primarily on A/B testing and experimentation analytics. Its SDK does local evaluation, but:

- **No push updates**: Growthbook refreshes features on a configurable interval. There is no SSE push channel.
- **No React Native JSI**: Growthbook's React Native SDK is a JavaScript wrapper — no native bridge.
- **Experiment-first**: Growthbook's targeting and rollout model is designed around experiments and metrics. Sidekick's model is simpler and faster for pure flag gating.

### vs. Split.io / Optimizely (commercial)

These platforms are experimentation and analytics suites, not feature flag systems. They carry the cost and complexity of full analytics pipelines, data warehouses, and statistical engines — all of which you pay for whether you need them or not. If you want a flag system, not an experimentation platform, Sidekick is a fraction of the complexity and cost.

### Summary

| Feature | Sidekick | LaunchDarkly | Unleash OSS | Flagsmith OSS | Growthbook OSS |
|---|---|---|---|---|---|
| Self-hosted | ✅ | ❌ (SaaS) | ✅ | ✅ | ✅ |
| Local evaluation (all SDKs) | ✅ | ✅ | Partial | Paid only | ✅ |
| SSE push (not polling) | ✅ | ✅ | ❌ | ❌ | ❌ |
| Sub-microsecond eval | ✅ | ❌ | ❌ | ❌ | ❌ |
| React Native JSI (native bridge) | ✅ | ❌ | ❌ | ❌ | ❌ |
| Browser WebAssembly SDK | ✅ | ❌ | ❌ | ❌ | ❌ |
| Flutter FFI SDK | ✅ | ❌ | ❌ | ❌ | ❌ |
| Rust evaluation engine | ✅ | ❌ | ❌ | ❌ | ❌ |
| Zero user data leaves your infra | ✅ | ❌ | ✅ | ✅ | ✅ |
| Pricing | Free | $$$$ | Free | Free | Free |

---

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                  Control Plane                        │
│                                                       │
│  REST API (Axum)          PostgreSQL                  │
│  POST /api/flags    ───→  flags table (source of truth)│
│  PATCH /api/flags/:key    (JSONB, indexed by key)     │
│  DELETE /api/flags/:key                               │
│                                ↓                      │
│  Redis pub/sub ←── flag change published on write     │
│       ↓                                               │
│  GET /stream (SSE) ←── Redis subscriber per connection│
└──────────────────────────────────────────────────────┘
                │ SSE (persistent HTTP connection)
                │ 1. subscribe to Redis
                │ 2. send "connected" event
                │ 3. stream full flag state
                │ 4. forward live deltas
     ┌──────────┼────────────────────┐
     ▼          ▼                    ▼
 Node.js SDK  Browser SDK    React Native SDK    Flutter SDK
 (NAPI/Rust)  (WASM/Rust)    (JSI/C++/Rust)     (FFI/Rust)
     │              │                │                │
     └──────────────┴────────────────┴────────────────┘
                          │
                     isEnabled(flagKey, userId, attributes)
                     → in-process lookup (< 1 µs, no network)
```

### Evaluation Flow

1. SDK opens a persistent SSE connection to `/stream`
2. Server sends `connected` event — SDK clears its local cache
3. Server streams all current flags as `UPSERT` events — SDK rebuilds cache
4. Server forwards live deltas as they arrive from Redis
5. `isEnabled()` evaluates entirely in local memory:
   - If `is_enabled = false` → `false` (global kill-switch)
   - If any targeting rule matches → `true` (bypasses rollout)
   - Otherwise: `MurmurHash3(flag_key:user_key) % 100 < rollout_percentage`

Rollouts are **deterministic and sticky** — the same user always gets the same result for the same flag, without any server-side session state.

### Race-free bootstrap

The SSE handler subscribes to Redis **before** streaming the current flag state. This means any write that happens between the client connecting and the initial dump is captured in the Redis queue — no update is ever silently dropped.

---

## Core Concepts

### Feature Flags

A flag is the fundamental unit. Each flag has:

| Field | Type | Description |
|---|---|---|
| `key` | `string` | Unique identifier used in code (`isEnabled('my_flag', ...)`) |
| `is_enabled` | `bool` | Global kill-switch. `false` short-circuits all evaluation. |
| `rollout_percentage` | `0–100 \| null` | What percentage of users see this flag. `null` = 100%. |
| `description` | `string \| null` | Human-readable label for dashboards. |
| `rules` | `TargetingRule[]` | Targeting rules evaluated before rollout. |

### Targeting Rules

Rules let you enable a flag for specific users regardless of rollout percentage — useful for internal beta testers, specific organizations, or premium plan users.

```json
{
  "attribute": "email",
  "operator": "EndsWith",
  "values": ["@acme.com", "@beta.acme.com"]
}
```

**Available operators:**

| Operator | Matches when |
|---|---|
| `Equals` | attribute exactly equals any value in the list |
| `NotEquals` | attribute does not equal any value in the list |
| `Contains` | attribute contains any value as a substring |
| `StartsWith` | attribute starts with any value |
| `EndsWith` | attribute ends with any value |

Rules are evaluated in order. The **first match** enables the flag for that user — the rollout percentage is ignored.

### Rollout Percentage

When no rule matches, rollout uses `MurmurHash3(flagKey:userKey) % 100`. This gives:

- **Sticky assignments** — same user always gets the same bucket for the same flag
- **Independent assignments** — a user's bucket for `flag_a` is independent of their bucket for `flag_b`
- **No server state** — the bucket is computed from the key alone, no database needed
- **Uniform distribution** — MurmurHash3 distributes uniformly, so 50% rollout gives close to 50/50 across large populations

### Real-Time Propagation

Flag changes propagate in this sequence:

```
Write API → Postgres (durable) → Redis pub/sub (broadcast)
                                       ↓
                          All server instances receive delta
                                       ↓
                          Push via SSE to all connected SDKs
                                       ↓
                          SDK updates in-memory cache atomically
```

Propagation latency from API write to SDK cache update is typically **< 50 ms** — bounded by Redis pub/sub latency and TCP, not polling intervals.

### Offline Resilience

If the SSE connection drops (network outage, server restart, mobile background), the SDK **continues evaluating against the last-known flag state**. When the connection is re-established, the server replays the full current state so the cache self-heals to the authoritative truth.

---

## Getting Started

### Prerequisites

- Docker and Docker Compose
- Rust 1.85+ (for building from source)

### Run Locally

```bash
# 1. Start Postgres and Redis
docker compose up -d

# 2. Build and run the server
cargo run -p server
```

The server starts on `http://localhost:3000`.

Auth is disabled by default in local dev (no `SDK_KEY` set). Set `SDK_KEY` for any environment beyond localhost.

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `postgres://sidekick:password@localhost/sidekick` | PostgreSQL connection string |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection string |
| `SDK_KEY` | *(unset — auth disabled)* | Bearer token required on all API requests. Set this in production. |

The `flags` table is auto-created on startup if it does not exist.

---

## API Reference

All endpoints require `Authorization: Bearer <SDK_KEY>` when `SDK_KEY` is set.

### Flags

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/flags` | List all flags (served from in-memory cache) |
| `POST` | `/api/flags` | Create or fully replace a flag |
| `GET` | `/api/flags/:key` | Get a single flag by key |
| `PATCH` | `/api/flags/:key` | Partially update a flag (JSON merge) |
| `DELETE` | `/api/flags/:key` | Delete a flag and broadcast removal |
| `GET` | `/stream` | SSE stream — initial state + live deltas |

### Flag Schema

```json
{
  "key": "dark_mode",
  "is_enabled": true,
  "rollout_percentage": 50,
  "description": "Enable dark mode for 50% of users",
  "rules": [
    {
      "attribute": "email",
      "operator": "EndsWith",
      "values": ["@acme.com"]
    }
  ]
}
```

### PATCH — Partial Update

`PATCH /api/flags/:key` accepts any subset of the flag fields. Only provided fields are overwritten; omitted fields keep their current values.

```bash
# Toggle a flag off without touching rules or rollout
curl -X PATCH https://flags.yourcompany.com/api/flags/dark_mode \
  -H "Authorization: Bearer sk_prod_abc123" \
  -H "Content-Type: application/json" \
  -d '{"is_enabled": false}'
```

### SSE Delta Messages

```json
{ "type": "UPSERT", "flag": { "key": "dark_mode", "is_enabled": true, ... } }
{ "type": "DELETE", "key": "dark_mode" }
```

The stream also emits a `connected` event (data: `"true"`) at the start of every connection, including reconnects. SDKs use this as the signal to clear their local cache before the server replays the full state.

---

## SDK Usage

### Node.js

```bash
npm install @sidekick/nodejs-sdk
```

```javascript
import { SidekickClient } from '@sidekick/nodejs-sdk';

const client = new SidekickClient(
  'https://flags.yourcompany.com',
  'sk_prod_abc123'
);

// Opens SSE — no REST bootstrap call needed.
// Server streams the full flag state on connect.
await client.init();

// Sub-microsecond. No network. Works offline.
const enabled = client.isEnabled('dark_mode', userId, {
  email: 'user@acme.com',
  plan: 'pro',
  country: 'US',
});

// Shutdown: close the SSE connection
client.close();
```

The Node.js SDK uses a **NAPI native module** — the evaluation engine is compiled Rust running inside the Node.js process. There is no JavaScript flag evaluation.

---

### Browser (WebAssembly)

```bash
npm install @sidekick/browser-sdk
```

```javascript
import { SidekickBrowserClient } from '@sidekick/browser-sdk';

const client = new SidekickBrowserClient(
  'https://flags.yourcompany.com',
  'sk_prod_abc123'
);

// Initialises the Wasm module and opens SSE.
await client.init();

const enabled = client.isEnabled('dark_mode', userId, { country: 'US' });
```

The browser SDK evaluates inside **WebAssembly** compiled from the same Rust core as all other SDKs. Auth is sent via `?sdk_key=` query parameter because the browser `EventSource` API does not support custom headers.

---

### React Native (JSI)

```bash
npm install @sidekick/react-native-sdk
cd ios && pod install
```

**Native setup (once, per project):**

`ios/YourApp/SidekickModule.mm`:
```objc
#import <React/RCTBridgeModule.h>
#import "SidekickJSI.h"

@implementation SidekickModule
RCT_EXPORT_MODULE()
- (void)setBridge:(RCTBridge *)bridge {
  auto jsiRuntime = (facebook::jsi::Runtime *)bridge.runtime;
  sidekick::installSidekickJSI(*jsiRuntime);
}
@end
```

`android/app/CMakeLists.txt`:
```cmake
add_library(sidekick_rn SHARED IMPORTED)
set_target_properties(sidekick_rn PROPERTIES
    IMPORTED_LOCATION "${CMAKE_SOURCE_DIR}/jni/${ANDROID_ABI}/libsidekick_rn.so")
add_library(sidekick SHARED cpp/SidekickJSI.cpp)
target_link_libraries(sidekick sidekick_rn jsi)
```

**Usage:**
```javascript
import { SidekickMobileClient } from '@sidekick/react-native-sdk';

const client = new SidekickMobileClient(
  'https://flags.yourcompany.com',
  'sk_prod_abc123'
);

await client.init();

// Crosses JS → C++ JSI → Rust synchronously.
// No async, no bridge overhead, no network.
const enabled = client.isEnabled('new_checkout', userId, {
  plan: 'pro',
  country: 'US',
});
```

`isEnabled()` is a **synchronous** call that crosses the JSI bridge directly into the Rust binary — no promise, no async/await, no `NativeModules` overhead.

---

### Flutter (FFI)

Add to `pubspec.yaml`:
```yaml
dependencies:
  sidekick_flutter:
    path: path/to/sidekick/sdks/flutter/dart
```

```dart
import 'package:sidekick_flutter/sidekick_flutter.dart';

final client = SidekickFlutterClient(
  serverUrl: 'https://flags.yourcompany.com',
  sdkKey: 'sk_prod_abc123',
);

await client.init();

final enabled = client.isEnabled(
  'dark_mode',
  userId,
  {'plan': 'pro', 'country': 'US'},
);

// Cleanup
client.close();
```

The Flutter SDK uses `dart:ffi` to call directly into the compiled Rust library (`libsidekick_flutter.so` on Android, statically linked on iOS). The SSE stream is implemented as a chunked HTTP stream parsed in Dart — no third-party SSE package needed.

---

## Deployment

### Docker

```bash
docker build -t sidekick-server .

docker run -p 3000:3000 \
  -e DATABASE_URL=postgres://user:pass@your-db/sidekick \
  -e REDIS_URL=redis://your-redis:6379 \
  -e SDK_KEY=sk_prod_abc123 \
  sidekick-server
```

The Dockerfile uses a multi-stage build: Rust compiler stage → slim Debian runtime. The final image contains only the compiled binary and its system dependencies.

---

### AWS

A reference production setup on AWS:

```
Route 53
    ↓ HTTPS
Application Load Balancer
    ↓
ECS Fargate (sidekick-server)    ←→   ElastiCache Redis (pub/sub)
    ↓                                        ↑
RDS Postgres (flags table)       ←── writes ─┘
```

**Critical ALB setting:** Set the ALB idle timeout to at least **300 seconds**. The default 60 s will terminate SSE connections before the 15 s keep-alive fires enough times. Sidekick SDKs reconnect automatically, but frequent disconnects increase bootstrap traffic.

```bash
aws elbv2 modify-load-balancer-attributes \
  --load-balancer-arn arn:aws:elasticloadbalancing:... \
  --attributes Key=idle_timeout.timeout_seconds,Value=300
```

**Horizontal scaling:** Multiple server instances work automatically. Each instance subscribes to the same Redis pub/sub channel, so a write to any instance propagates to all SDKs connected to any instance.

**Security group:** Expose port 3000 only to the ALB security group. The `SDK_KEY` env var gates all API and SSE access.

---

### CI/CD

| Workflow | Trigger | Action |
|---|---|---|
| `deploy-server.yml` | Push to `main` affecting `core/` or `server/` | Build Docker image → push to registry → rolling ECS deploy |
| `publish-sdks.yml` | GitHub release tag created | Publish `@sidekick/nodejs-sdk` and `@sidekick/browser-sdk` to npm; Flutter SDK to pub.dev |

---

## Development

### Running Tests

```bash
cargo test
```

Tests cover:
- `test_flag_disabled` — global kill-switch returns false regardless of rules
- `test_flag_rollout` — 50% rollout distributes within ±5% across 1000 users
- `test_flag_rules_match` — rule match bypasses 0% rollout
- `test_murmurhash3_x86_32` — hash consistency with known test vectors

### Building the Node.js SDK

```bash
cd sdks/nodejs
npm install
npm run build   # runs napi build --release
```

### Building the Browser SDK (WASM)

```bash
cd sdks/browser
wasm-pack build --target web
```

Output is written to `dist/`. Import `dist/sidekick.js` in the browser SDK's `index.js`.

### Building the React Native FFI Library

```bash
# Android (cross-compile per ABI)
cargo build -p react-native --target aarch64-linux-android --release
cargo build -p react-native --target x86_64-linux-android --release

# iOS
cargo build -p react-native --target aarch64-apple-ios --release
cargo build -p react-native --target x86_64-apple-ios --release  # simulator
```

### Building the Flutter FFI Library

```bash
# Android
cargo build -p flutter --target aarch64-linux-android --release

# iOS (static lib)
cargo build -p flutter --target aarch64-apple-ios --release
```

Copy the output `.so` / `.a` into your Flutter project's native directories and reference them from `CMakeLists.txt` (Android) or your Xcode project (iOS).

---

## Repository Structure

```
sidekick/
├── core/                        # Shared Rust evaluation library
│   └── src/
│       ├── lib.rs               # Module exports
│       ├── store.rs             # Thread-safe DashMap flag cache + list_flags()
│       ├── evaluator.rs         # Targeting rules + rollout evaluation + tests
│       └── hashing.rs           # MurmurHash3 (deterministic, cross-platform)
│
├── server/                      # REST API + SSE control plane
│   └── src/
│       ├── main.rs              # Startup: DB, Redis, SDK_KEY, auth middleware
│       ├── state.rs             # Shared AppState (db, redis, store, sdk_key)
│       ├── auth.rs              # Bearer token middleware (header + query param)
│       ├── stream.rs            # SSE: subscribe-first, full state dump, live deltas
│       └── api/
│           └── flags.rs         # CRUD + PATCH endpoints
│
├── sdks/
│   ├── nodejs/                  # Native NAPI module
│   │   ├── src/lib.rs           # upsert_flag, delete_flag, clear_store, is_enabled
│   │   └── index.js             # JS wrapper: SSE, cache management, clear-on-reconnect
│   │
│   ├── browser/                 # WebAssembly module
│   │   ├── src/lib.rs           # wasm-bindgen exports
│   │   └── index.js             # ES module wrapper: SSE, ?sdk_key= auth
│   │
│   ├── react-native/            # JSI native module
│   │   ├── src/lib.rs           # extern "C" FFI exports (global LazyLock store)
│   │   ├── cpp/
│   │   │   ├── sidekick_core.h  # C header (matches Rust exports exactly)
│   │   │   └── SidekickJSI.cpp  # JSI installer: JSON.stringify bridge, 4 methods
│   │   └── index.js             # JS wrapper: SSE, clear-on-reconnect, deleteFlag
│   │
│   └── flutter/                 # Dart FFI module
│       ├── src/lib.rs           # extern "C" FFI exports (global LazyLock store)
│       └── dart/
│           ├── sidekick_bindings.dart  # Raw dart:ffi typedefs + DynamicLibrary loader
│           ├── sidekick_flutter.dart   # High-level client: SSE, chunked HTTP, isEnabled
│           └── pubspec.yaml            # ffi: ^2.1.0, http: ^1.2.0
│
├── Dockerfile                   # Multi-stage: rust:slim → debian:slim
├── docker-compose.yml           # Local dev: Postgres + Redis
└── Cargo.toml                   # Workspace: core, server, nodejs, browser, flutter, react-native
```

---

## Dependencies

| Crate | Purpose |
|---|---|
| `axum 0.8` | Web framework — routing, extractors, middleware |
| `tokio` | Async runtime |
| `sqlx` | Async PostgreSQL driver |
| `redis` | Redis client with async pub/sub |
| `dashmap` | Lock-free concurrent HashMap for the flag store |
| `murmur3` | MurmurHash3 for deterministic rollout bucketing |
| `async-stream` | Stream macro for SSE generator |
| `serde / serde_json` | Serialization |
| `tracing` | Structured logging |
| `napi / napi-derive` | Node.js native addon bindings |
| `wasm-bindgen` | WebAssembly JS interop |
| `serde-wasm-bindgen` | Serde support for WASM JsValue |
| `console_error_panic_hook` | Rust panics → browser console errors |

---

## License

MIT
