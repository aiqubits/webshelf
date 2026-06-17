//! Snowflake ID 生成器。
//!
//! 基于 Twitter Snowflake 算法，生成 64 位整数 ID：
//! - 1 bit 符号位 (始终为 0)
//! - 41 bits 毫秒时间戳 (自定义 epoch，可用 ~69 年)
//! - 10 bits worker ID (支持 1024 个节点)
//! - 12 bits 序列号 (每毫秒每节点 4096 个 ID)
//!
//! # 线程安全
//! 使用 `AtomicI64`（无锁方案）实现，无需 Mutex。
//!
//! # Worker 协调
//! 通过数据库表 `snowflake_worker` 实现自动协调：
//! - 启动时注册一个 worker_id（0-1023），退出时删除
//! - 定时心跳保活（每 10 秒）
//! - 启动时清理过期（30 秒无心跳）的 worker 条目
//! - 多节点部署时自动分配不重复的 worker_id

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 自定义 epoch：2024-01-01T00:00:00.000Z（毫秒）
const CUSTOM_EPOCH: u64 = 1_704_067_200_000;

/// Worker ID 位数
const WORKER_ID_BITS: u64 = 10;
/// 序列号位数
const SEQUENCE_BITS: u64 = 12;

/// Worker ID 最大值
const MAX_WORKER_ID: u64 = (1 << WORKER_ID_BITS) - 1;
/// 序列号最大值
const MAX_SEQUENCE: u64 = (1 << SEQUENCE_BITS) - 1;

/// Worker ID 左移位数
const WORKER_ID_SHIFT: u64 = SEQUENCE_BITS;
/// 时间戳左移位数
const TIMESTAMP_SHIFT: u64 = SEQUENCE_BITS + WORKER_ID_BITS;

/// 心跳间隔（秒）
const HEARTBEAT_INTERVAL_SECS: u64 = 10;
/// 过期阈值（秒）：超过此时间无心跳的 worker 视为过期
const STALE_WORKER_SECS: i64 = 30;

// ──────────────────────────────────────────────
//  SnowflakeGenerator
// ──────────────────────────────────────────────

/// Snowflake ID 生成器（无锁方案）。
///
/// 使用 `AtomicI64` 代替 `Mutex`：
/// - `last_timestamp`：最后一次生成 ID 的时间戳（毫秒）
/// - `sequence`：当前毫秒内的序列号
pub struct SnowflakeGenerator {
    worker_id: u64,
    sequence: AtomicI64,
    last_timestamp: AtomicI64,
}

impl SnowflakeGenerator {
    /// 创建一个新的生成器。
    ///
    /// # Panics
    /// 如果 `worker_id` 超出范围 [0, 1023] 会 panic。
    pub fn new(worker_id: u64) -> Self {
        assert!(
            worker_id <= MAX_WORKER_ID,
            "Snowflake worker ID must be between 0 and {}",
            MAX_WORKER_ID
        );
        Self {
            worker_id,
            sequence: AtomicI64::new(0),
            last_timestamp: AtomicI64::new(0),
        }
    }

    /// 生成下一个 Snowflake ID。
    ///
    /// ## 无锁算法说明
    ///
    /// 1. **同一毫秒**：`fetch_add` 原子递增序列号，若未耗尽则返回 ID。
    /// 2. **新毫秒**：`compare_exchange` 竞争更新 `last_timestamp`，
    ///    胜出者重置序列号为 1 并返回序列号 0。
    ///    失败者回退到同一毫秒路径，通过 `fetch_add` 获取后续序列号。
    /// 3. **时钟回拨**：自旋等待到时间追上。
    /// 4. **序列号耗尽**：自旋等待下一毫秒。
    ///
    /// 所有路径最终都通过 `fetch_add` 或 CAS 胜出者路径返回唯一 ID，
    /// 保证了在任意并发情况下不会产生重复的 (timestamp, sequence) 组合。
    pub fn generate(&self) -> i64 {
        loop {
            let last_ts = self.last_timestamp.load(Ordering::Acquire);
            let current_ts = Self::current_timestamp();

            if current_ts < last_ts {
                // 时钟回拨：自旋等待到 last_timestamp
                tracing::warn!(
                    "Snowflake clock moved backward from {} to {}, waiting...",
                    last_ts,
                    current_ts
                );
                while Self::current_timestamp() < last_ts {
                    std::hint::spin_loop();
                }
                continue;
            }

            if current_ts == last_ts {
                // 同一毫秒：原子递增序列号
                let seq = self.sequence.fetch_add(1, Ordering::AcqRel);
                if (seq as u64) < MAX_SEQUENCE {
                    return compose_id(current_ts, self.worker_id, seq as u64);
                }
                // 序列号耗尽，等待下一毫秒
                while Self::current_timestamp() <= current_ts {
                    std::hint::spin_loop();
                }
                continue;
            }

            // current_ts > last_ts：新毫秒，竞争更新时间戳
            if self
                .last_timestamp
                .compare_exchange(last_ts, current_ts, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                // 胜出：重置 sequence 为新毫秒纪元，防止上一毫秒已耗尽的序列号
                // 溢出到 worker_id 位（见 MAX_SEQUENCE 定义）。
                // store(0) 后紧跟 fetch_add(1) 原子地声明序列号 0，
                // 同一毫秒内后续 fetch_add 会获得不同的序列号，不会产生重复。
                self.sequence.store(0, Ordering::Release);
                let seq = self.sequence.fetch_add(1, Ordering::AcqRel);
                return compose_id(current_ts, self.worker_id, seq as u64);
            }
            // 竞争失败，回退重试
        }
    }

    /// 获取当前系统时间的毫秒数。
    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System clock before UNIX epoch")
            .as_millis() as i64
    }
}

/// 组合 Snowflake ID。
fn compose_id(timestamp: i64, worker_id: u64, sequence: u64) -> i64 {
    let ts_part = (timestamp as u64 - CUSTOM_EPOCH) << TIMESTAMP_SHIFT;
    let wk_part = worker_id << WORKER_ID_SHIFT;
    (ts_part | wk_part | sequence) as i64
}

// ──────────────────────────────────────────────
//  DB Worker 注册
// ──────────────────────────────────────────────

/// Snowflake worker 注册记录。
#[derive(Debug, Clone)]
pub struct WorkerRecord {
    pub worker_id: i16,
}

/// Worker 生命周期句柄。
///
/// 持有此句柄时，后台任务会定期发送心跳。
/// 句柄被 `Drop` 时自动注销 worker 并停止心跳。
#[derive(Debug)]
pub struct WorkerHandle {
    worker_id: i16,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl WorkerHandle {
    pub fn worker_id(&self) -> i16 {
        self.worker_id
    }
}

/// 在数据库中注册一个新的 Snowflake worker。
///
/// 1. 清理 30 秒无心跳的过期条目
/// 2. 遍历 0..1024 查找可用 worker_id
/// 3. INSERT + UNIQUE 约束防竞态
/// 4. 返回注册成功的 worker_id
async fn register_worker(db: &DatabaseConnection) -> Result<WorkerRecord, SnowflakeError> {
    let host = hostname();
    let pid = std::process::id() as i32;

    // 清理过期条目
    clean_stale_workers(db).await?;

    // 查询已被占用的 worker_id
    use crate::repositories::snowflake_worker::Column;
    let used: Vec<i16> = crate::repositories::snowflake_worker::Entity::find()
        .select_only()
        .column(Column::WorkerId)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| SnowflakeError::Db(e.to_string()))?;

    // 查找最低的可用 ID
    for wid in 0..=MAX_WORKER_ID as i16 {
        if used.contains(&wid) {
            continue;
        }
        // 尝试插入
        match try_insert_worker(db, wid, &host, pid).await {
            Ok(()) => return Ok(WorkerRecord { worker_id: wid }),
            Err(SnowflakeError::WorkerIdTaken) => continue, // 并发冲突，试下一个
            Err(e) => return Err(e),
        }
    }

    Err(SnowflakeError::NoAvailableWorker)
}

/// 尝试插入一条 worker 记录。
async fn try_insert_worker(
    db: &DatabaseConnection,
    worker_id: i16,
    host: &str,
    pid: i32,
) -> Result<(), SnowflakeError> {
    use crate::repositories::snowflake_worker::ActiveModel;
    use sea_orm::ActiveModelTrait;

    let now = chrono::Utc::now();
    let model = ActiveModel {
        worker_id: sea_orm::Set(worker_id),
        host: sea_orm::Set(host.to_string()),
        pid: sea_orm::Set(pid),
        heartbeat: sea_orm::Set(now),
        created_at: sea_orm::Set(now),
    };

    match model.insert(db).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // UniqueConstraintViolation (23505)：worker_id 已被占用
            if let Some(sea_orm::SqlErr::UniqueConstraintViolation(_)) = e.sql_err() {
                return Err(SnowflakeError::WorkerIdTaken);
            }
            Err(SnowflakeError::Db(e.to_string()))
        }
    }
}

/// 清理过期的 worker 条目（30 秒无心跳）。
async fn clean_stale_workers(db: &DatabaseConnection) -> Result<(), SnowflakeError> {
    use crate::repositories::snowflake_worker::Column;
    use sea_orm::EntityTrait;

    let deadline = chrono::Utc::now() - chrono::Duration::seconds(STALE_WORKER_SECS);

    crate::repositories::snowflake_worker::Entity::delete_many()
        .filter(Column::Heartbeat.lt(deadline))
        .exec(db)
        .await
        .map_err(|e| SnowflakeError::Db(e.to_string()))?;

    Ok(())
}

/// 发送心跳（更新 heartbeat 时间戳）。
async fn heartbeat(db: &DatabaseConnection, worker_id: i16) -> Result<(), SnowflakeError> {
    use crate::repositories::snowflake_worker::Column;
    use sea_orm::EntityTrait;

    crate::repositories::snowflake_worker::Entity::update_many()
        .col_expr(
            Column::Heartbeat,
            sea_orm::sea_query::Expr::value(chrono::Utc::now()),
        )
        .filter(Column::WorkerId.eq(worker_id))
        .exec(db)
        .await
        .map_err(|e| SnowflakeError::Db(e.to_string()))?;

    Ok(())
}

/// 注销 worker（删除数据库记录）。
async fn deregister_worker(db: &DatabaseConnection, worker_id: i16) -> Result<(), SnowflakeError> {
    use crate::repositories::snowflake_worker::Column;
    use sea_orm::EntityTrait;

    crate::repositories::snowflake_worker::Entity::delete_many()
        .filter(Column::WorkerId.eq(worker_id))
        .exec(db)
        .await
        .map_err(|e| SnowflakeError::Db(e.to_string()))?;

    tracing::info!("Snowflake worker {} deregistered", worker_id);
    Ok(())
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ──────────────────────────────────────────────
//  错误类型
// ──────────────────────────────────────────────

/// Snowflake 相关错误。
#[derive(Debug, thiserror::Error)]
pub enum SnowflakeError {
    #[error("No available Snowflake worker ID (all 1024 slots are occupied)")]
    NoAvailableWorker,

    #[error("Worker ID already taken (concurrent registration)")]
    WorkerIdTaken,

    #[error("Database error: {0}")]
    Db(String),
}

// ──────────────────────────────────────────────
//  全局初始化和公共 API
// ──────────────────────────────────────────────

/// 全局 Snowflake 生成器。
static GLOBAL_GENERATOR: std::sync::OnceLock<SnowflakeGenerator> = std::sync::OnceLock::new();

/// 初始化 Snowflake 系统。
///
/// 在数据库上注册一个 worker，创建全局生成器，并启动后台心跳任务。
/// 返回的 `WorkerHandle` 必须保持存活直到服务器关闭，`Drop` 时自动注销。
///
/// # Idempotent
/// 如果已经初始化过，不会重复注册，而是返回一个 no-op 句柄（stop_tx: None）。
/// 这允许集成测试在同一个进程中多次调用 `create_test_app()`。
pub async fn init(db: &DatabaseConnection) -> Result<WorkerHandle, SnowflakeError> {
    tracing::info!("Initializing Snowflake ID generator...");

    // Idempotent: if already initialized, return a no-op handle.
    // This allows integration tests to call init() multiple times
    // across separate create_test_app() invocations.
    if GLOBAL_GENERATOR.get().is_some() {
        tracing::warn!("Snowflake already initialized, returning no-op handle");
        return Ok(WorkerHandle {
            worker_id: 0,
            stop_tx: None,
        });
    }

    let record = register_worker(db).await?;
    let generator = SnowflakeGenerator::new(record.worker_id as u64);

    GLOBAL_GENERATOR
        .set(generator)
        .map_err(|_| SnowflakeError::Db("Snowflake already initialized".to_string()))?;

    tracing::info!(
        "Snowflake generator initialized with worker_id={}",
        record.worker_id
    );

    // 启动心跳任务
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
    let db_clone = db.clone();
    let worker_id = record.worker_id;

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
        // 首次心跳立即执行
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = heartbeat(&db_clone, worker_id).await {
                        tracing::warn!("Snowflake heartbeat failed: {e}");
                    }
                }
                _ = &mut stop_rx => {
                    tracing::info!("Snowflake worker {} shutting down", worker_id);
                    if let Err(e) = deregister_worker(&db_clone, worker_id).await {
                        tracing::warn!("Snowflake deregister failed: {e}");
                    }
                    break;
                }
            }
        }
    });

    Ok(WorkerHandle {
        worker_id: record.worker_id,
        stop_tx: Some(stop_tx),
    })
}

/// 使用全局生成器生成一个新的 Snowflake ID。
///
/// # Panics
/// 如果 `init()` 尚未被调用，会 panic。
pub fn generate_id() -> i64 {
    GLOBAL_GENERATOR
        .get()
        .expect("Snowflake not initialized — call snowflake::init(db) during server startup")
        .generate()
}

// ──────────────────────────────────────────────
//  SnowflakeId 包装类型
// ──────────────────────────────────────────────

/// Snowflake ID 的新类型包装，提供 JSON 序列化为字符串（避免 JS 精度丢失）。
///
/// 在服务端内部表现为 `i64`，但对外 API 序列化为字符串。
/// 这确保 WASM 前端 (JavaScript Number 仅能安全表示 2^53 以内的整数)
/// 能无损接收 Snowflake ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnowflakeId(i64);

impl SnowflakeId {
    /// 创建 SnowflakeId 实例。
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// 获取内部 i64 值。
    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl From<i64> for SnowflakeId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<SnowflakeId> for i64 {
    fn from(id: SnowflakeId) -> Self {
        id.0
    }
}

impl std::fmt::Display for SnowflakeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SnowflakeId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<i64>().map(Self)
    }
}

/// 序列化为 JSON 字符串
impl Serialize for SnowflakeId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

/// 从 JSON 字符串或数字反序列化
impl<'de> Deserialize<'de> for SnowflakeId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SnowflakeIdVisitor;
        impl serde::de::Visitor<'_> for SnowflakeIdVisitor {
            type Value = SnowflakeId;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a snowflake ID as string or number")
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<SnowflakeId, E> {
                Ok(SnowflakeId(value))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<SnowflakeId, E> {
                if value > i64::MAX as u64 {
                    return Err(serde::de::Error::custom(format!(
                        "Snowflake ID overflow: {} exceeds i64::MAX",
                        value
                    )));
                }
                Ok(SnowflakeId(value as i64))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<SnowflakeId, E> {
                value
                    .parse::<i64>()
                    .map(SnowflakeId)
                    .map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_any(SnowflakeIdVisitor)
    }
}

// ──────────────────────────────────────────────
//  测试
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_unique_ids() {
        let generator = SnowflakeGenerator::new(0);
        let id1 = generator.generate();
        let id2 = generator.generate();
        assert_ne!(id1, id2);
        assert!(id1 > 0);
        assert!(id2 > 0);
    }

    #[test]
    fn test_id_monotonically_increasing() {
        let generator = SnowflakeGenerator::new(0);
        let mut prev = generator.generate();
        for _ in 0..10_000 {
            let cur = generator.generate();
            assert!(cur > prev, "IDs must be monotonically increasing");
            prev = cur;
        }
    }

    #[test]
    fn test_different_worker_ids() {
        let gen0 = SnowflakeGenerator::new(0);
        let gen1 = SnowflakeGenerator::new(1);
        let id0 = gen0.generate();
        let id1 = gen1.generate();
        assert_ne!(id0, id1);
    }

    #[test]
    fn test_snowflake_id_display() {
        let id = SnowflakeId::new(12345);
        assert_eq!(id.to_string(), "12345");
    }

    #[test]
    fn test_snowflake_id_from_str() {
        let id: SnowflakeId = "67890".parse().unwrap();
        assert_eq!(id.as_i64(), 67890);
    }

    #[test]
    fn test_snowflake_id_serde_json_string() {
        let id = SnowflakeId::new(1234567890);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"1234567890\"");
    }

    #[test]
    fn test_snowflake_id_deser_json_string() {
        let id: SnowflakeId = serde_json::from_str("\"1234567890\"").unwrap();
        assert_eq!(id.as_i64(), 1234567890);
    }

    #[test]
    fn test_snowflake_id_deser_json_number() {
        let id: SnowflakeId = serde_json::from_str("1234567890").unwrap();
        assert_eq!(id.as_i64(), 1234567890);
    }

    #[test]
    fn test_snowflake_id_from_i64() {
        let id: SnowflakeId = 42.into();
        assert_eq!(id.as_i64(), 42);
    }

    #[test]
    fn test_snowflake_id_into_i64() {
        let id = SnowflakeId::new(99);
        let val: i64 = id.into();
        assert_eq!(val, 99);
    }

    #[test]
    fn test_concurrent_generation() {
        use std::sync::Arc;
        use std::thread;

        let generator = Arc::new(SnowflakeGenerator::new(0));
        let mut handles = vec![];

        for _ in 0..8 {
            let g = generator.clone();
            handles.push(thread::spawn(move || {
                let mut ids = Vec::with_capacity(1000);
                for _ in 0..1000 {
                    ids.push(g.generate());
                }
                ids
            }));
        }

        let mut all_ids: Vec<i64> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();

        all_ids.sort();
        all_ids.dedup();

        // 8000 个 ID 全部唯一
        assert_eq!(all_ids.len(), 8000);
    }
}
