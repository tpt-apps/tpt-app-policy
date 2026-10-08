//! `tpt-policy serve`: HTTP interface for policy evaluation (spec §8.4).
//!
//! Endpoints:
//!
//! - `POST /v1/evaluate`: body is the input JSON; response is the evaluation JSON.
//! - `GET /healthz`: `{"status": "ok"}`.
//!
//! If a token is set, `POST /v1/evaluate` requires `Authorization: Bearer <token>`.
//! Routing lives in [`route`], which takes no sockets, so it can be tested directly.

use std::io::Read;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};
use tpt_policy_core::{evaluate, Policy};

/// Largest request body accepted, in bytes.
pub const MAX_BODY_BYTES: u64 = 1024 * 1024;

/// Serve evaluations of `policy` on `addr`. Blocks until the process is stopped.
pub fn serve(policy: Policy, addr: &str, token: Option<String>) -> Result<(), String> {
    let server = Server::http(addr).map_err(|e| e.to_string())?;
    println!(
        "listening on http://{addr} (policy {} version {}, auth {})",
        policy.name,
        policy.version,
        if token.is_some() { "required" } else { "off" }
    );

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let path = request.url().to_string();
        let auth = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str().to_string());

        let mut body = String::new();
        let read = request
            .as_reader()
            .take(MAX_BODY_BYTES + 1)
            .read_to_string(&mut body);

        let (status, reply) = match read {
            Err(_) => (
                400,
                error_body(
                    "invalid body",
                    "the request body is not valid UTF-8 text",
                    "send the input as UTF-8 JSON",
                ),
            ),
            Ok(n) if n as u64 > MAX_BODY_BYTES => (
                413,
                error_body(
                    "body too large",
                    &format!("the body is over {MAX_BODY_BYTES} bytes"),
                    "split the input or raise the limit in a future release",
                ),
            ),
            Ok(_) => route(
                &policy,
                token.as_deref(),
                &method,
                &path,
                auth.as_deref(),
                &body,
            ),
        };

        let response = Response::from_string(reply)
            .with_status_code(status)
            .with_header(json_header());
        // A client that disconnected mid-request is not an error for the server.
        let _ = request.respond(response);
    }
    Ok(())
}

/// Pure request handler. Returns `(status code, JSON body)`.
pub fn route(
    policy: &Policy,
    token: Option<&str>,
    method: &Method,
    path: &str,
    auth: Option<&str>,
    body: &str,
) -> (u16, String) {
    match (method, path) {
        (Method::Get, "/healthz") => (200, json!({"status": "ok"}).to_string()),
        (Method::Post, "/v1/evaluate") => {
            if let Some(expected) = token {
                let presented = auth.and_then(|a| a.strip_prefix("Bearer "));
                if !presented.is_some_and(|p| constant_time_eq(p.as_bytes(), expected.as_bytes())) {
                    return (
                        401,
                        error_body(
                            "unauthorised",
                            "the Authorization header is missing or the token is wrong",
                            "send 'Authorization: Bearer <token>' with the configured token",
                        ),
                    );
                }
            }
            match serde_json::from_str::<Value>(body) {
                Ok(input) => {
                    let evaluation = evaluate(policy, &input);
                    (
                        200,
                        serde_json::to_string_pretty(&evaluation)
                            .expect("evaluation is always serialisable"),
                    )
                }
                Err(e) => (
                    400,
                    error_body(
                        "invalid JSON",
                        &e.to_string(),
                        "send the input as a JSON object in the request body",
                    ),
                ),
            }
        }
        (_, "/healthz") | (_, "/v1/evaluate") => (
            405,
            error_body(
                "method not allowed",
                &format!("{method} is not supported on {path}"),
                "use GET for /healthz and POST for /v1/evaluate",
            ),
        ),
        _ => (
            404,
            error_body(
                "not found",
                &format!("there is no endpoint at {path}"),
                "use POST /v1/evaluate or GET /healthz",
            ),
        ),
    }
}

fn error_body(what: &str, why: &str, fix: &str) -> String {
    json!({"error": {"what": what, "why": why, "fix": fix}}).to_string()
}

fn json_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header is valid")
}

/// Compares secrets without returning early on the first differing byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_policy_core::parse_policy;

    fn policy() -> Policy {
        parse_policy(
            "policy: t\nversion: \"2.0.0\"\nrules:\n  - id: big\n    when:\n      amount: {gt: 100}\n    then:\n      decision: approval_required\n",
        )
        .unwrap()
    }

    #[test]
    fn health_check() {
        let (status, body) = route(&policy(), None, &Method::Get, "/healthz", None, "");
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"status":"ok"}"#);
    }

    #[test]
    fn evaluate_returns_decision() {
        let (status, body) = route(
            &policy(),
            None,
            &Method::Post,
            "/v1/evaluate",
            None,
            r#"{"amount": 500}"#,
        );
        assert_eq!(status, 200);
        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["decision"], "approval_required");
        assert_eq!(json["policy"]["version"], "2.0.0");
    }

    #[test]
    fn bad_json_is_400() {
        let (status, body) = route(
            &policy(),
            None,
            &Method::Post,
            "/v1/evaluate",
            None,
            "{nope",
        );
        assert_eq!(status, 400);
        assert!(body.contains("invalid JSON"));
    }

    #[test]
    fn token_is_enforced() {
        let p = policy();
        let (missing, _) = route(
            &p,
            Some("s3cret"),
            &Method::Post,
            "/v1/evaluate",
            None,
            "{}",
        );
        assert_eq!(missing, 401);

        let (wrong, _) = route(
            &p,
            Some("s3cret"),
            &Method::Post,
            "/v1/evaluate",
            Some("Bearer nope"),
            "{}",
        );
        assert_eq!(wrong, 401);

        let (ok, _) = route(
            &p,
            Some("s3cret"),
            &Method::Post,
            "/v1/evaluate",
            Some("Bearer s3cret"),
            "{}",
        );
        assert_eq!(ok, 200);
    }

    #[test]
    fn health_check_needs_no_token() {
        let (status, _) = route(
            &policy(),
            Some("s3cret"),
            &Method::Get,
            "/healthz",
            None,
            "",
        );
        assert_eq!(status, 200);
    }

    #[test]
    fn wrong_method_and_unknown_path() {
        let (wrong_method, _) = route(&policy(), None, &Method::Get, "/v1/evaluate", None, "");
        assert_eq!(wrong_method, 405);

        let (unknown, _) = route(&policy(), None, &Method::Get, "/nope", None, "");
        assert_eq!(unknown, 404);
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
