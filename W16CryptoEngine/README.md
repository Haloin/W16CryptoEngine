# predmarket

A binary prediction market where people trade YES/NO contracts on real-world events. Prices are probabilities — if a contract trades at 0.65, the market thinks there's a 65% chance that event happens. When the market resolves, winning contracts pay out 1 unit per share.

Built by Haloin — middmaxmbollar@gmail.com

MIT License

---

## How it works

The stack is split into a few moving parts:

- **Rust API** (axum) — handles auth, order validation, balances, and settlement
- **C++ matching engine** — keeps the order book in memory, matches by price-time priority, writes a WAL for crash recovery
- **NATS** — passes fills from the engine back to the API
- **PostgreSQL** — all durable state (users, orders, balances, positions)

```
Client  <--HTTP/JSON + WS-->  Rust API  <--NATS-->  C++ Engine
                                                    |
                                                Postgres
```

The engine replays its WAL on startup before connecting to NATS, so you won't lose sequence numbers across restarts. Sequence allocation is done in Postgres with `INSERT ... ON CONFLICT DO UPDATE`, which keeps things monotonic even if the box crashes mid-trade.

---

## What you need

- Rust 1.78+
- GCC or Clang with C++20
- CMake 3.20+
- PostgreSQL 16
- NATS Server 2.10+ with JetStream
- `libnats` (Debian/Ubuntu: `libnats-dev`)
- `nlohmann-json` (Debian/Ubuntu: `nlohmann-json3-dev`)

---

## Quick start with Docker

```bash
cp .env.example .env
# edit POSTGRES_PASSWORD and JWT_SECRET

cd infra/docker
docker compose up -d
```

## Running locally

Start the deps:

```bash
nats-server -js &
pg_ctl start
```

Build and run the engine:

```bash
cd engine
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
PREDMARKET_WAL_DIR=/tmp/predmarket-wal ./build/predmarket-engine &
```

Then the API:

```bash
cd api
export PREDMARKET__DATABASE__URL="postgres://predmarket:password@localhost/predmarket"
export PREDMARKET__JWT__SECRET="$(bash ../scripts/gen_secret.sh)"
cargo run --release --bin api
```

---

## API overview

### Auth

| Method | Path | Auth | Description |
|--------|------|------|--------------|
| POST | `/v1/auth/register` | — | Sign up, get a JWT |
| POST | `/v1/auth/login` | — | Log in, get a JWT |
| GET | `/v1/auth/balance` | User | See available vs reserved balance |

### Markets

| Method | Path | Auth | Description |
|--------|------|------|--------------|
| GET | `/v1/markets` | — | List markets. Optional `?status=open\|paused\|settled\|cancelled` |
| POST | `/v1/markets` | Admin | Create a new market |
| GET | `/v1/markets/:id` | — | Get a single market |
| POST | `/v1/markets/:id/pause` | Admin | Stop trading temporarily |
| POST | `/v1/markets/:id/reopen` | Admin | Resume trading |
| POST | `/v1/markets/:id/settle` | Admin | Resolve it. Body: `{"outcome":"yes"\|"no"}` |

### Orders

| Method | Path | Auth | Description |
|--------|------|------|--------------|
| POST | `/v1/markets/:id/orders` | User | Place an order. Supports `Idempotency-Key` header |
| GET | `/v1/orders` | User | Your orders. Optional `?market_id=&limit=` |
| GET | `/v1/orders/:id` | User | Get one order |
| DELETE | `/v1/orders/:id` | User | Cancel it |

### Withdrawals

| Method | Path | Auth | Description |
|--------|------|------|--------------|
| POST | `/v1/withdrawals` | User | Request a withdrawal. Funds get held immediately |
| GET | `/v1/withdrawals` | User | Your withdrawals. Optional `?status=pending\|approved\|rejected` |
| GET | `/v1/admin/withdrawals` | Admin | See everyone's withdrawals |
| POST | `/v1/admin/withdrawals/:id/approve` | Admin | Release hold, funds leave the platform |
| POST | `/v1/admin/withdrawals/:id/reject` | Admin | Return held funds to user's balance |

### Admin

| Method | Path | Auth | Description |
|--------|------|------|--------------|
| POST | `/v1/admin/deposits` | Admin | Credit a user's balance. Supports `Idempotency-Key` |

### Real-time

| Protocol | Path | Description |
|----------|------|--------------|
| WebSocket | `/v1/markets/:id/ws` | Live order book depth. Pushes JSON on every fill. Sends a ping every 30s if nothing's happening |

### System

| Method | Path | Description |
|--------|------|--------------|
| GET | `/healthz` | Health check, returns 200 |
| GET | `/metrics` | Prometheus metrics |

---

## Placing an order

```json
POST /v1/markets/:id/orders
Idempotency-Key: <uuid>

{
  "side": "buy",
  "kind": "limit",
  "price": 0.65,
  "quantity": 10.0
}
```

Price is a float between 0 and 1. Quantity is in shares (min 0.001). When you submit, the required funds are reserved. Cancel the order and they come back.

---

## WebSocket depth feed

Connect to `ws://host/v1/markets/:id/ws`. On each fill you'll get:

```json
{
  "market_id": "...",
  "type": "fill",
  "price": 6500,
  "quantity": 1000000
}
```

The server sends `{"type":"ping"}` every 30 seconds when quiet. Just respond with a standard WebSocket pong.

---

## Settlement

```json
POST /v1/markets/:id/settle
{"outcome": "yes"}
```

This reads net positions, computes payouts, and credits balances in a single transaction. Market status flips to `settled` atomically.

---

## Balance lifecycle

```
deposit -> available -> reserved (on order) -> deducted (on fill)
                     ^                        |
                     |____ on cancel _________|
```

Withdrawals work the same way — request moves funds from `available` to `reserved`. Approval releases them out of the system. Rejection puts them back.

---

## A few things to know

**Idempotency:** If you pass `Idempotency-Key: <uuid>` on order placement or admin deposits, the first request with that key runs and stores its response. Any retries with the same key return the cached response without re-executing. Good for network hiccups.

**Rate limits:** Auth endpoints are capped at 10 req/min per IP. Order endpoints at 60 req/sec per IP.

**WAL recovery:** The C++ engine replays its write-ahead log before it subscribes to NATS, so you don't get duplicate or lost fills after a restart.

---

## Scripts

```bash
bash scripts/gen_secret.sh          # generate a JWT secret
DATABASE_URL=... ADMIN_EMAIL=... bash scripts/create_admin.sh
```

---

## License

MIT — Copyright (c) 2024 Haloin <middmaxmbollar@gmail.com>
