-- 系统尚未上线，无需兼容旧数据，所有字段直接定义在 CREATE TABLE 中。
-- 如需增加新字段，直接在此文件修改即可，无需编写单独的 ALTER TABLE 迁移脚本。
-- Create users table if not exists
CREATE TABLE IF NOT EXISTS users (
    id BIGINT PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    name VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'user'
        CHECK (role IN ('user', 'admin', 'system')),
    token_version INTEGER NOT NULL DEFAULT 1,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    verification_code_hash VARCHAR(255),
    verification_code_expires_at TIMESTAMPTZ,
    verification_code_sent_at TIMESTAMPTZ,
    verification_failed_attempts INTEGER NOT NULL DEFAULT 0,
    password_reset_token_hash VARCHAR(255),
    password_reset_expires_at TIMESTAMPTZ,
    password_reset_sent_at TIMESTAMPTZ,
    password_reset_failed_attempts INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    balance BIGINT NOT NULL DEFAULT 0
);

-- Add balance column for existing dev databases that pre-date this column
ALTER TABLE users ADD COLUMN IF NOT EXISTS balance BIGINT NOT NULL DEFAULT 0;

-- Idempotent ALTERs for existing dev databases that pre-date the password-reset columns.
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_reset_token_hash VARCHAR(255);
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_reset_expires_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_reset_sent_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_reset_failed_attempts INTEGER NOT NULL DEFAULT 0;

-- Create index on email for faster lookups
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Create index on created_at for pagination
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at DESC);

-- Snowflake worker ID registry for automatic worker coordination.
-- Each server instance registers here on startup to get a unique worker_id (0-1023).
-- Stale entries (heartbeat older than 30s) are cleaned up during registration.
CREATE TABLE IF NOT EXISTS snowflake_worker (
    worker_id SMALLINT PRIMARY KEY CHECK (worker_id >= 0 AND worker_id < 1024),
    host TEXT NOT NULL DEFAULT '',
    pid INTEGER NOT NULL DEFAULT 0,
    heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
