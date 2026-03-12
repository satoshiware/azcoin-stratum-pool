//! Persistence layer. DB models, repositories, migrations. PostgreSQL-oriented.
//! Implementations minimal—stubs where needed for bootstrap.

pub mod models;
pub mod repositories;

pub use models::*;
pub use repositories::*;
