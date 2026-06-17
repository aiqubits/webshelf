use sea_orm::entity::prelude::*;

/// Snowflake worker registration record.
///
/// Each server instance registers a row on startup, keeps it alive via heartbeat,
/// and removes it on shutdown. `worker_id` (0-1023) is the Snowflake node ID.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "snowflake_worker")]
pub struct Model {
    /// Unique Snowflake worker ID (0-1023)
    #[sea_orm(primary_key, auto_increment = false)]
    pub worker_id: i16,

    /// Hostname of the worker instance
    pub host: String,

    /// Process ID of the worker instance
    pub pid: i32,

    /// Last heartbeat timestamp (updated every 10s)
    pub heartbeat: DateTimeUtc,

    /// Registration timestamp
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
