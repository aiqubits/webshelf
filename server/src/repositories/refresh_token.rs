use sea_orm::entity::prelude::*;

/// Refresh token database entity model.
///
/// Stores only the SHA-256 hash of the refresh token — the raw token is
/// delivered to the client via an httpOnly cookie and never persisted server-side.
/// Each user may have multiple rows (one per device/session), but in practice
/// the login flow deletes old tokens before inserting a new one.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "refresh_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,

    pub user_id: i64,

    /// SHA-256 hash of the raw refresh token
    #[sea_orm(unique)]
    pub token_hash: String,

    pub expires_at: DateTimeUtc,

    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl ActiveModelBehavior for ActiveModel {}
