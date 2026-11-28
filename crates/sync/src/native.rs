//! A blocking HTTP transport for native builds (Android, desktop, tests),
//! with rustls and the Mozilla root certificates (webpki-roots).

use std::time::Duration;

use crate::{HttpRequest, HttpResponse, Transport};

pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::with_timeout(Duration::from_secs(60))
    }
}

impl UreqTransport {
    /// With a shorter limit per request (the Home Assistant bridge, which
    /// must not keep Home Assistant waiting on an unreachable server).
    pub fn with_timeout(timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent }
    }
}

impl Transport for UreqTransport {
    fn send(&mut self, req: HttpRequest) -> Result<HttpResponse, String> {
        let mut b = ureq::http::Request::builder()
            .method(req.method.as_str())
            .uri(&req.url);
        for (k, v) in &req.headers {
            b = b.header(k, v);
        }
        let request = b.body(req.body).map_err(|e| e.to_string())?;
        let mut res = self.agent.run(request).map_err(|e| e.to_string())?;
        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();
        let body = res
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}
