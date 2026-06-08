use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{Error, HttpMessage};
use futures::future::{ok, LocalBoxFuture, Ready};
use log::debug;
use std::rc::Rc;

use super::protocol::{
    HEADER_AGENT_ID, HEADER_EXECUTION_MODE, HEADER_INTENT, HEADER_LEDGER_MODE,
    HEADER_PERFORMANCE_TIER, HEADER_PRIVACY_MODE, HEADER_PROTOCOL, HEADER_SESSION_ID,
    HEADER_TRACE_ID, HEADER_VERSION, HTTPA_PROTOCOL_NAME, HTTPA_VERSION,
};

#[derive(Debug, Clone)]
pub struct HttpaContext {
    pub version: Option<String>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub intent: Option<String>,
    pub trace_id: String,
    pub execution_mode: Option<String>,
    pub privacy_mode: Option<String>,
    pub performance_tier: Option<String>,
    pub ledger_mode: Option<String>,
    pub is_httpa_request: bool,
}

pub struct HttpaMiddleware;

impl<S, B> Transform<S, ServiceRequest> for HttpaMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = HttpaMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(HttpaMiddlewareService {
            service: Rc::new(service),
        })
    }
}

pub struct HttpaMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for HttpaMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        Box::pin(async move {
            let headers = req.headers();
            let is_httpa_request =
                headers.contains_key(HEADER_PROTOCOL) || req.path().starts_with("/httpa");
            let trace_id = headers
                .get(HEADER_TRACE_ID)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()[..16].to_string());

            let ctx = HttpaContext {
                version: headers
                    .get(HEADER_VERSION)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                agent_id: headers
                    .get(HEADER_AGENT_ID)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                session_id: headers
                    .get(HEADER_SESSION_ID)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                intent: headers
                    .get(HEADER_INTENT)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                trace_id,
                execution_mode: headers
                    .get(HEADER_EXECUTION_MODE)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                privacy_mode: headers
                    .get(HEADER_PRIVACY_MODE)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                performance_tier: headers
                    .get(HEADER_PERFORMANCE_TIER)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                ledger_mode: headers
                    .get(HEADER_LEDGER_MODE)
                    .and_then(|value| value.to_str().ok())
                    .map(String::from),
                is_httpa_request,
            };

            if ctx.is_httpa_request {
                debug!(
                    "HTTPA request: agent={:?} intent={:?} trace={}",
                    ctx.agent_id, ctx.intent, ctx.trace_id
                );
            }

            req.extensions_mut().insert(ctx.clone());

            let mut res = svc.call(req).await?;
            if ctx.is_httpa_request {
                let response_headers = res.headers_mut();
                response_headers.insert(
                    HeaderName::from_static("x-httpa-protocol"),
                    HeaderValue::from_static(HTTPA_PROTOCOL_NAME),
                );
                response_headers.insert(
                    HeaderName::from_static("x-httpa-version"),
                    HeaderValue::from_static(HTTPA_VERSION),
                );
                if let Ok(trace_header) = HeaderValue::from_str(&ctx.trace_id) {
                    response_headers
                        .insert(HeaderName::from_static("x-httpa-trace-id"), trace_header);
                }
            }

            Ok(res)
        })
    }
}
