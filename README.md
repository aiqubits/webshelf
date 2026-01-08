# WebShelf

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![Rust-Agent](https://img.shields.io/badge/webshelf-release-yellow)](https://crates.io/crates/webshelf)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/aiqubits/webshelf/pulls)
[![Live Demo](https://img.shields.io/badge/demo-live-success)](https://www.openpick.org/webshelf)

**The best way to develop your web service with one click.**

WebShelf is a production-ready Rust web framework built on Axum, providing a complete backend scaffold with authentication, database integration, distributed locking, and comprehensive middleware support.

## ✨ Features

- 🔐 **JWT Authentication** - Secure token-based authentication with Argon2 password hashing
- 🗄️ **Database Integration** - PostgreSQL support via SeaORM with async operations
- 🔒 **Distributed Locking** - Redis-based distributed locks for scalable services
- 🛡️ **Middleware Stack** - Panic capture, CORS, tracing, and authentication layers
- ✅ **Input Validation** - Request validation with email and password rules
- 📝 **Structured Logging** - Tracing-based logging with configurable levels
- ⚙️ **Flexible Configuration** - TOML-based config with CLI argument overrides
- 🧪 **Testing Support** - Unit tests and integration test framework
- 🚦 **RESTful API** - Complete CRUD operations for user management
- 📦 **Production Ready** - Error handling, compression, and graceful shutdown

## 📋 Requirements

- Rust 1.92 or higher
- PostgreSQL 12+
- Redis 6+ (for distributed locking)

## 🚀 Quick Start

### 1. Clone and Setup

```bash
git clone https://github.com/aiqubits/webshelf.git
cd webshelf
```

### 2. Configure Database

Create a PostgreSQL database:

```bash
creatdb webshelf
```

Copy and edit configuration:

```bash
cp config.toml.example config.toml
# Edit config.toml with your database credentials
```

### 3. Run the Server

```bash
cargo run
```

The server will start on `http://0.0.0.0:3000` by default.

## 🔧 Configuration

### Configuration File (`config.toml`)

```toml
# Database connection
database_url = "postgres://postgres:password@localhost:5432/webshelf"

# Redis for distributed locking
redis_url = "redis://localhost:6379"

# JWT settings
jwt_secret = "your-super-secret-key-change-in-production"
jwt_expiry_seconds = 3600

# Server settings
[server]
host = "0.0.0.0"
port = 3000

# Rate limiting
[rate_limit]
per_second = 2
burst_size = 5
```

### Command Line Arguments

```bash
webshelf [OPTIONS]

Options:
  -H, --host <HOST>              Server bind address [default: 0.0.0.0]
  -P, --port <PORT>              Server port [default: 3000]
  -E, --env <ENV>                Environment [default: development]
  -C, --config <CONFIG>          Configuration file path [default: config.toml]
  -L, --log-level <LOG_LEVEL>    Log level [default: info]
  -h, --help                     Print help
  -V, --version                  Print version
```

Example:

```bash
cargo run -- --host 127.0.0.1 --port 8080 --log-level debug
```

## 📚 API Endpoints

### Authentication

#### Register User
```http
POST /api/public/auth/register
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "Password123",
  "name": "User Name"
}
```

#### Login
```http
POST /api/public/auth/login
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "Password123"
}
```

Response:
```json
{
  "access_token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
  "user": {
    "id": "uuid",
    "email": "user@example.com",
    "name": "User Name"
  }
}
```

### User Management

#### Create User
```http
POST /api/users
Content-Type: application/json

{
  "email": "newuser@example.com",
  "password": "SecurePass123",
  "name": "New User"
}
```

#### Get User
```http
GET /api/users/{id}
```

#### Update User
```http
PUT /api/users/{id}
Content-Type: application/json

{
  "email": "updated@example.com",
  "name": "Updated Name",
  "role": "admin"
}
```

#### Delete User
```http
DELETE /api/users/{id}
```

#### List Users (with pagination)
```http
GET /api/users?page=1&per_page=10
```

#### Health Check
```http
GET /api/health
```

Response:
```json
{
  "status": "ok",
  "version": "0.0.1"
}
```

## 🏗️ Project Structure

```
webshelf/
├── src/
│   ├── middleware/          # Middleware components
│   │   ├── auth.rs          # JWT authentication
│   │   ├── panic.rs         # Panic capture
│   │   └── mod.rs
│   ├── models/              # Data models
│   │   ├── user.rs          # User entity
│   │   └── mod.rs
│   ├── routes/              # API route handlers
│   │   ├── api.rs           # User CRUD endpoints
│   │   ├── auth.rs          # Authentication endpoints
│   │   └── mod.rs
│   ├── services/            # Business logic
│   │   ├── auth.rs          # Authentication service
│   │   ├── user.rs          # User service
│   │   ├── lock.rs          # Distributed locking
│   │   └── mod.rs
│   ├── utils/               # Utilities
│   │   ├── config.rs        # Configuration loading
│   │   ├── error.rs         # Error types
│   │   ├── logger.rs        # Logging setup
│   │   ├── password.rs      # Password hashing
│   │   ├── validator.rs     # Input validation
│   │   └── mod.rs
│   ├── lib.rs               # Library exports
│   └── main.rs              # Application entry
├── tests/
│   └── integration_tests.rs # Integration tests
├── Cargo.toml               # Dependencies
├── config.toml              # Configuration
└── README.md                # This file
```

## 🧪 Testing

### Run Unit Tests

```bash
cargo test
```

### Run Integration Tests

**Note:** Integration tests require PostgreSQL and Redis to be running.

```bash
# Start PostgreSQL and Redis first
cargo test --test integration_tests
```

### Test Coverage

- ✅ Password hashing and verification
- ✅ Input validation (email, password)
- ✅ Configuration loading
- ✅ API endpoint integration tests
- ✅ User CRUD operations

## 🔐 Security Features

- **Password Hashing**: Argon2 algorithm with salt
- **JWT Tokens**: Secure token generation and validation
- **Input Validation**: Email format and password strength checks
- **CORS**: Configurable cross-origin resource sharing
- **Panic Recovery**: Graceful error handling without server crashes

### Password Requirements

- Minimum 8 characters
- At least one lowercase letter
- At least one uppercase letter
- At least one digit

## 📦 Dependencies

### Core
- **axum** - Web framework
- **tokio** - Async runtime
- **sea-orm** - ORM for PostgreSQL
- **redis** - Distributed locking

### Authentication
- **jsonwebtoken** - JWT handling
- **argon2** - Password hashing

### Utilities
- **serde** - Serialization
- **validator** - Input validation
- **tracing** - Structured logging
- **anyhow/thiserror** - Error handling

See [Cargo.toml](Cargo.toml) for the complete dependency list.

## 🛠️ Development

### Run in Development Mode

```bash
cargo run -- --env development --log-level debug
```

### Build for Production

```bash
cargo build --release
```

### Run Production Binary

```bash
./target/release/webshelf --config prod.config.toml
```

## 📊 Middleware Stack

Middleware execution order (innermost to outermost):

1. **Panic Capture** - Catches panics and returns 500 errors
2. **Authentication** - JWT token validation (for protected routes)
3. **Trace** - Request/response logging
4. **CORS** - Cross-origin resource sharing

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 👥 Authors

- **aiqubits** - *The first complete version* - [aiqubits@hotmail.com](mailto:aiqubits@hotmail.com)

## 📝 Changelog

### Current Version

- ✅ Registration and login functionality
- ✅ JWT authentication
- ✅ Input validation for email and password formats
- ✅ User CRUD operations
- ✅ PostgreSQL integration
- ✅ Redis distributed locking
- ✅ Comprehensive middleware stack, including panic recovery, authentication, tracing, and CORS support for robust web services
- ✅ Utility functions for configuration loading, error handling, logging, and password hashing
- ✅ Integration tests
