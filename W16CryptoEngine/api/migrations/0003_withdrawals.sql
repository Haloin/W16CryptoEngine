CREATE TYPE withdrawal_status AS ENUM ('pending', 'approved', 'rejected');

CREATE TABLE withdrawals (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id),
    amount      BIGINT NOT NULL CHECK (amount > 0),
    status      withdrawal_status NOT NULL DEFAULT 'pending',
    reference   TEXT,
    reviewed_by UUID REFERENCES users(id),
    reviewed_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_withdrawals_user   ON withdrawals(user_id);
CREATE INDEX idx_withdrawals_status ON withdrawals(status);

CREATE TABLE withdrawal_holds (
    withdrawal_id UUID NOT NULL REFERENCES withdrawals(id) ON DELETE CASCADE,
    user_id       UUID NOT NULL REFERENCES users(id),
    amount        BIGINT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (withdrawal_id)
);
