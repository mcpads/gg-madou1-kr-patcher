//! BPS 패치 생성/적용 (배포용 JP→KO 델타).

mod apply;
mod create;
pub mod vli;

pub use apply::apply;
pub use create::create;
