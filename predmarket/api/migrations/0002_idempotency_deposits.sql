CREATE TABLE idempotency_keys (
    key        TEXT NOT NULL,
    user_id    UUID NOT NULL REFERENCES users(id),
    status     SMALLINT NOT NULL DEFAULT 0,
    response   JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now() + INTERVAL '24 hours',
    PRIMARY KEY (user_id, key)
);

CREATE INDEX idx_idempotency_expires ON idempotency_keys(expires_at);

CREATE TABLE deposits (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES users(id),
    amount     BIGINT NOT NULL CHECK (amount > 0),
    reference  TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_deposits_user ON deposits(user_id);

ALTER TABLE position_changes ADD COLUMN user_id_fk UUID REFERENCES users(id);
