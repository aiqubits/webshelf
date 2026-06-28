# wechat-api

Extensible WeChat Official Account API for **webshelf**.

Encapsulates the WeChat captcha-login flow into a
standalone, pluggable crate designed to grow with future WeChat features.

## Features

| Feature | Default | Description |
|---------|:-------:|-------------|
| `client` | ✅ | HTTP client for WeChat Platform API calls (`access_token`, customer-service messages) |
| `memory-store` | ❌ | In-memory store implementations (tests / single-instance dev) |
| `crypto-safe-mode` | ❌ | AES-CBC decryption for WeChat "safe mode" encrypted messages |

## Module map

| Module | Responsibility |
|--------|----------------|
| `config` | Account credentials & login-flow settings |
| `error` | Unified `WechatError` / `WechatResult` |
| `store` | Pluggable traits: `CaptchaStore`, `UserBindingStore`, `AccessTokenStore` |
| `crypto` | Signature verification + optional AES decryption |
| `message` | XML (de)serialization & reply builders |
| `callback` | Framework-agnostic GET/POST callback handling |
| `client` | WeChat Platform HTTP API client (`access_token`, custom messages) |
| `captcha` | Captcha generation, storage, one-shot verification |
| `handler` | Extensible `MessageHandler` trait + `HandlerChain` + default handlers |
| `login` | `LoginService` orchestrating captcha → user lookup |

## Login flow

```text
① User sends "验证码" to the official account
   → WeChat POST callback → parse_callback → CaptchaTriggerHandler
   → CaptchaService::generate → WechatClient::send_text_message
   → User receives code in WeChat

② User enters code on the web login page
   → POST /auth/wx-login { openid, code }
   → LoginService::verify_and_login
   → CaptchaService::verify_for_openid (one-shot consume)
   → UserBindingStore::find_user_by_openid
   → host app issues JWT for the returned user_id
```

## Integration with webshelf

1. Implement the store traits for webshelf's `CacheService` and DB layer.
2. Build `WechatClient` + `CaptchaService` + `LoginService` in `bootstrap`.
3. Wire `CaptchaTriggerHandler` into the WeChat callback route.
4. Call `LoginService::verify_and_login` from a new `/auth/wx-login` handler.

## Extensibility

Adding new WeChat capabilities later only requires a new module (e.g.
`pay.rs`, `template.rs`, `miniprogram/`) and optionally new store traits. The
existing core (`config`, `crypto`, `message`, `callback`) is reused unchanged.
