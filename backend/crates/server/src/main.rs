use std::fs::File;
use std::io::{self, BufReader, ErrorKind};

use actix_cors::Cors;
use actix_web::{
    App, HttpResponse, HttpServer, dev::Service, http::header, middleware::DefaultHeaders, web,
};
use astra_core::common::new_id;
use astra_server::{app_config, build_state};
use rustls::ServerConfig;
use tracing_subscriber::EnvFilter;

#[actix_web::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let state = build_state();
    let host = state.config.host.clone();
    let port = state.config.port;
    let cors_origins = state.config.cors_allowed_origins.clone();
    let tls_cert_path = state.config.tls_cert_path.clone();
    let tls_key_path = state.config.tls_key_path.clone();

    let server =
        HttpServer::new(move || {
            let cors = build_cors(&cors_origins);
            App::new()
                .app_data(web::Data::new(state.clone()))
                .app_data(web::JsonConfig::default().limit(1024 * 1024).error_handler(
                    |error, _req| {
                        actix_web::error::InternalError::from_response(
                            error,
                            HttpResponse::BadRequest().json(serde_json::json!({
                                "ok": false,
                                "error": "invalid json payload",
                            })),
                        )
                        .into()
                    },
                ))
                .wrap(
                    DefaultHeaders::new()
                        .add((header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
                        .add((header::X_FRAME_OPTIONS, "DENY"))
                        .add((header::REFERRER_POLICY, "no-referrer"))
                        .add((
                            header::HeaderName::from_static("x-httpa-transport"),
                            "https-preferred",
                        )),
                )
                .wrap_fn(|req, srv| {
                    let request_id = new_id("req");
                    let fut = srv.call(req);
                    async move {
                        let mut response = fut.await?;
                        response.headers_mut().insert(
                            header::HeaderName::from_static("x-request-id"),
                            header::HeaderValue::from_str(&request_id)
                                .unwrap_or_else(|_| header::HeaderValue::from_static("invalid")),
                        );
                        Ok(response)
                    }
                })
                .wrap(cors)
                .configure(app_config)
        })
        .bind((host.clone(), port))?;

    if let (Some(cert_path), Some(key_path)) = (tls_cert_path, tls_key_path) {
        server.bind_rustls_0_23((host, port + 1), load_rustls_config(&cert_path, &key_path)?)?
    } else {
        server
    }
    .run()
    .await
}

fn build_cors(allowed_origins: &[String]) -> Cors {
    let mut cors = Cors::default()
        .allow_any_header()
        .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
        .max_age(3600);
    for origin in allowed_origins {
        cors = cors.allowed_origin(origin);
    }
    cors
}

fn load_rustls_config(cert_path: &str, key_path: &str) -> io::Result<ServerConfig> {
    let cert_file = &mut BufReader::new(File::open(cert_path)?);
    let key_file = &mut BufReader::new(File::open(key_path)?);

    let cert_chain = rustls_pemfile::certs(cert_file)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?;
    let private_key = rustls_pemfile::private_key(key_file)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "missing private key"))?;

    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))
}
