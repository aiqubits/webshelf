//! 测试工具模块
//!
//! 提供 Wiremock Mock 服务器和测试辅助函数/夹具。

use client_api::{Client, ClientConfig};
use wiremock::MockServer;

/// 创建带 Mock 服务器的测试客户端。
///
/// 返回的 `Client` 自动指向 Mock 服务器的随机端口，
/// 无需启动真实后端即可测试所有 HTTP 交互。
pub async fn create_test_client() -> (Client, MockServer) {
    let mock_server = MockServer::start().await;
    let config = ClientConfig::new(mock_server.uri())
        .with_max_retries(0) // 集成测试中关闭重试以简化断言
        .with_timeout(10);
    let client = Client::new(config).expect("Failed to create test client");
    (client, mock_server)
}

/// 测试用常量夹具
#[allow(dead_code)]
pub mod fixtures {
    pub const TEST_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test_token.signature";
    pub const TEST_USER_ID: &str = "1903487293645824000";
    pub const TEST_EMAIL: &str = "test@example.com";
    pub const TEST_PASSWORD: &str = "SecurePass123!";
    pub const TEST_NAME: &str = "Test User";

    /// 构造一个标准的 UserResponse JSON 对象
    pub fn user_json(
        id: &str,
        email: &str,
        name: &str,
        role: &str,
        created_at: &str,
        updated_at: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "email": email,
            "name": name,
            "role": role,
            "created_at": created_at,
            "updated_at": updated_at,
        })
    }
}
