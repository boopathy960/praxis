use actix_web::web;

mod artifacts;
mod asc2;
mod assistant;
mod connectors;
mod httpa;
mod nexus;
mod os_guardian;
mod render;
mod research;
mod runtime;
mod sandbox;
mod system;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1")
            .configure(system::configure)
            .configure(asc2::configure)
            .configure(assistant::configure)
            .configure(artifacts::configure)
            .configure(connectors::configure)
            .configure(httpa::configure)
            .configure(nexus::configure)
            .configure(os_guardian::configure)
            .configure(render::configure)
            .configure(research::configure)
            .configure(sandbox::configure)
            .configure(runtime::configure),
    );
}
