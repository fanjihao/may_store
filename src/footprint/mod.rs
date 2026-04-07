pub mod models;
pub mod service;
pub mod routes;

use ntex::web;

pub fn footprint_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/footprint")
            .route("/check-permission", web::get().to(routes::check_permission))
            .route("/overview", web::get().to(routes::get_overview))
            .route("/groups", web::get().to(routes::get_groups))
            .route("/records", web::get().to(routes::get_records))
            .route("/records", web::post().to(routes::submit_record))
            .route("/records/{id}", web::get().to(routes::get_record))
            .route("/records/{id}", web::put().to(routes::update_record))
            .route("/records/{id}", web::delete().to(routes::delete_record))
            .route("/capacity/expand", web::post().to(routes::expand_capacity)),
    );
}
