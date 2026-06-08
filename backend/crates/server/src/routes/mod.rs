use actix_web::web;

pub mod v1;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api").configure(v1::configure));
}
