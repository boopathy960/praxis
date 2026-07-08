use actix_web::{HttpResponse, web};

pub mod v1;

/// The Genome OS console — a self-contained single-page app embedded in the
/// binary and served at the root, so the product UI ships with the server and
/// needs no separate build step or static host.
const CONSOLE_HTML: &str = include_str!("../console/genome_os.html");

/// The CEO Guardian security dashboard — a self-contained page that renders the
/// live posture, findings, remediations, and patrol history from the
/// `/api/v1/ceo/guardian/*` endpoints.
const GUARDIAN_HTML: &str = include_str!("../console/guardian.html");

async fn console() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(CONSOLE_HTML)
}

async fn guardian_console() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(GUARDIAN_HTML)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api").configure(v1::configure))
        .route("/", web::get().to(console))
        .route("/console", web::get().to(console))
        .route("/guardian", web::get().to(guardian_console));
}
