-- 系统尚未上线，无需兼容旧数据，所有字段直接定义在 CREATE TABLE 中。
-- 如需增加新字段，直接在此文件修改即可，无需编写单独的 ALTER TABLE 迁移脚本。
-- Create users table if not exists
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    name VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'user'
        CHECK (role IN ('user', 'admin', 'system')),
    token_version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create index on email for faster lookups
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Create index on created_at for pagination
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at DESC);
