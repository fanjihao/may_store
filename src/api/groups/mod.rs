// API - 双人组管理模块
// FSD.latest.md compliant - /api/groups/...

pub mod partner_invitations;
pub mod routes;

use ntex::web::ServiceConfig;

pub fn configure(cfg: &mut ServiceConfig) {
    routes::configure(cfg);
    partner_invitations::configure(cfg);
}
