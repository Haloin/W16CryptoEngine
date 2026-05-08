CREATE TYPE order_side AS ENUM ('buy', 'sell');
CREATE TYPE order_type AS ENUM ('limit', 'market');
CREATE TYPE order_status AS ENUM ('open', 'partial', 'filled', 'cancelled', 'rejected');
CREATE TYPE market_status AS ENUM ('open', 'paused', 'settled', 'cancelled');
CREATE TYPE outcome AS ENUM ('yes', 'no');

CREATE TABLE users (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email        TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    is_admin     BOOLEAN NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_users_email ON users(email);

CREATE TABLE balances (
    user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    available BIGINT NOT NULL DEFAULT 0 CHECK (available >= 0),
    reserved  BIGINT NOT NULL DEFAULT 0 CHECK (reserved >= 0),
    version   BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id)
);

CREATE TABLE markets (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title       VARCHAR(200) NOT NULL,
    description TEXT NOT NULL,
    status      market_status NOT NULL DEFAULT 'open',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolves_at TIMESTAMPTZ,
    settled_at  TIMESTAMPTZ,
    outcome     outcome
);

CREATE INDEX idx_markets_status ON markets(status);
CREATE INDEX idx_markets_resolves_at ON markets(resolves_at) WHERE resolves_at IS NOT NULL;

CREATE TABLE orders (
    id              UUID PRIMARY KEY,
    market_id       UUID NOT NULL REFERENCES markets(id),
    user_id         UUID NOT NULL REFERENCES users(id),
    side            order_side NOT NULL,
    kind            order_type NOT NULL,
    price           INTEGER CHECK (price > 0 AND price < 10000),
    quantity        BIGINT NOT NULL CHECK (quantity > 0),
    filled_quantity BIGINT NOT NULL DEFAULT 0 CHECK (filled_quantity >= 0),
    status          order_status NOT NULL DEFAULT 'open',
    sequence        BIGINT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_limit_has_price CHECK (kind != 'limit' OR price IS NOT NULL),
    CONSTRAINT chk_filled_lte_quantity CHECK (filled_quantity <= quantity)
);

CREATE INDEX idx_orders_market_id ON orders(market_id);
CREATE INDEX idx_orders_user_id ON orders(user_id);
CREATE INDEX idx_orders_market_status ON orders(market_id, status) WHERE status IN ('open', 'partial');

CREATE TABLE fills (
    id             UUID PRIMARY KEY,
    market_id      UUID NOT NULL REFERENCES markets(id),
    maker_order_id UUID NOT NULL REFERENCES orders(id),
    taker_order_id UUID NOT NULL REFERENCES orders(id),
    maker_user_id  UUID NOT NULL REFERENCES users(id),
    taker_user_id  UUID NOT NULL REFERENCES users(id),
    price          INTEGER NOT NULL CHECK (price > 0 AND price < 10000),
    quantity       BIGINT NOT NULL CHECK (quantity > 0),
    aggressor      order_side NOT NULL,
    sequence       BIGINT NOT NULL,
    filled_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_fills_market_id ON fills(market_id);
CREATE INDEX idx_fills_maker_order ON fills(maker_order_id);
CREATE INDEX idx_fills_taker_order ON fills(taker_order_id);
CREATE UNIQUE INDEX idx_fills_sequence ON fills(market_id, sequence);

CREATE TABLE position_changes (
    id         BIGSERIAL PRIMARY KEY,
    fill_id    UUID NOT NULL REFERENCES fills(id),
    user_id    UUID NOT NULL REFERENCES users(id),
    market_id  UUID NOT NULL REFERENCES markets(id),
    delta      BIGINT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_poschanges_user_market ON position_changes(user_id, market_id);

CREATE TABLE audit_log (
    id         BIGSERIAL PRIMARY KEY,
    user_id    UUID REFERENCES users(id),
    action     TEXT NOT NULL,
    entity     TEXT NOT NULL,
    entity_id  TEXT,
    meta       JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_user ON audit_log(user_id);
CREATE INDEX idx_audit_entity ON audit_log(entity, entity_id);

CREATE TABLE engine_sequences (
    market_id UUID NOT NULL REFERENCES markets(id),
    last_seq  BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (market_id)
);

CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_orders_updated_at
    BEFORE UPDATE ON orders
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
