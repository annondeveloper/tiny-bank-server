-- Create users table
-- This migration is version-controlled and managed by sqlx-cli.
CREATE TABLE IF NOT EXISTS users
(
    id
    UUID
    PRIMARY
    KEY,
    account_number
    TEXT
    NOT
    NULL
    UNIQUE,
    ifsc_code
    TEXT
    NOT
    NULL,
    bank_name
    TEXT
    NOT
    NULL,
    branch
    TEXT
    NOT
    NULL,
    address
    TEXT,
    city
    TEXT,
    state_code
    TEXT,
    routing_no
    TEXT,
    created_at
    TIMESTAMPTZ
    NOT
    NULL
    DEFAULT
    NOW
(
)
    );
