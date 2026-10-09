//! `tpt-ai-guard serve`: HTTP interface for action decisions.
//!
//! Endpoints:
//!
//! - `POST /v1/decide`: body is the action request JSON; response is the verdict JSON.
//! - `GET /healthz`: `{"status": "ok"}`.
//!
//! If a token is set, `POST /v1/decide` requires `Authorization: Bearer <token>`.
//! Routing lives in [`route`], which takes no sockets, so it can be tested directly.

use std::io::Read;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};
use tpt_ai_guard::{decide_with_grants, ActionPolicy, Grants};

/// Largest request body accepted, in bytes.
pub const MAX_BODY_BYTES: u64 = 1024 * 1024;

/// Serve decisions for `policy` (and optional `grants`) on `addr`. Blocks until stopped.
pub fn serve(
    policy: ActionPolicy,
    grants: Option<Grants>,
    addr: &str,
    token: Option<String>,
) -> Result<(), String> {
    let server = Server::http(addr).map_err(|e| e.to_string())?;
    println!(
        "listening on http://{addr} (policy {} version {}, grants {}, auth {})",
        policy.name,
        policy.version,
        if grants.is_some() { "on" } else { "off" },
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
                    "send the request as UTF-8 JSON",
                ),
            ),
            Ok(n) if n as u64 > MAX_BODY_BYTES => (
                413,
                error_body(
                    "body too large",
                    &format!("the body is over {MAX_BODY_BYTES} bytes"),
                    "send a smaller request",
                ),
            ),
            Ok(_) => route(
                &policy,
                grants.as_ref(),
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
    policy: &ActionPolicy,
    grants: Option<&Grants>,
    token: Option<&str>,
    method: &Method,
    path: &str,
    auth: Option<&str>,
    body: &str,
) -> (u16, String) {
    match (method, path) {
        (Method::Get, "/healthz") => (200, json!({"status": "ok"}).to_string()),
        (Method::Post, "/v1/decide") => {
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
                Ok(request @ Value::Object(_)) => {
                    match decide_with_grants(policy, grants, &request) {
                        Ok(verdict) => (
                            200,
                            serde_json::to_string_pretty(&verdict)
                                .expect("verdict is always serialisable"),
                        ),
                        Err(why) => (
                            400,
                            error_body(
                                "invalid request",
                                &why,
                                "send a JSON object with a string 'action' field",
                            ),
                        ),
                    }
                }
                Ok(_) => (
                    400,
                    error_body(
                        "invalid request",
                        "the body is JSON but not an object",
                        "wrap the fields in { }",
                    ),
                ),
                Err(e) => (
                    400,
                    error_body(
                        "invalid JSON",
                        &e.to_string(),
                        "send the request as a JSON object in the body",
                    ),
                ),
            }
        }
        (_, "/healthz") | (_, "/v1/decide") => (
            405,
            error_body(
                "method not allowed",
                &format!("{method} is not supported on {path}"),
                "use GET for /healthz and POST for /v1/decide",
            ),
        ),
        _ => (
            404,
            error_body(
                "not found",
                &format!("there is no endpoint at {path}"),
                "use POST /v1/decide or GET /healthz",
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
    use tpt_ai_guard::parse_action_policy;

    fn policy() -> ActionPolicy {
        parse_action_policy("rules:\n  - action: lookup\n    decision: allow\n").unwrap()
    }

    #[test]
    fn health_check() {
        let (status, body) = route(&policy(), None, None, &Method::Get, "/healthz", None, "");
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"status":"ok"}"#);
    }

    #[test]
    fn decide_returns_verdict() {
        let (status, body) = route(
            &policy(),
            None,
            None,
            &Method::Post,
            "/v1/decide",
            None,
            r#"{"action": "lookup"}"#,
        );
        assert_eq!(status, 200);
        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["decision"], "ALLOW");
    }

    #[test]
    fn grants_are_checked_over_http() {
        let grants = tpt_ai_guard::parse_grants("fs_read: [\"/data\"]\n").unwrap();
        let (status, body) = route(
            &policy(),
            Some(&grants),
            None,
            &Method::Post,
            "/v1/decide",
            None,
            r#"{"action": "lookup", "path": "/etc/passwd"}"#,
        );
        assert_eq!(status, 200);
        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["decision"], "DENY");
    }

    #[test]
    fn missing_action_is_400() {
        let (status, body) = route(
            &policy(),
            None,
            None,
            &Method::Post,
            "/v1/decide",
            None,
            "{}",
        );
        assert_eq!(status, 400);
        assert!(body.contains("invalid request"));
    }

    #[test]
    fn bad_json_and_non_object_are_400() {
        let (bad, _) = route(
            &policy(),
            None,
            None,
            &Method::Post,
            "/v1/decide",
            None,
            "{nope",
        );
        assert_eq!(bad, 400);
        let (list, _) = route(
            &policy(),
            None,
            None,
            &Method::Post,
            "/v1/decide",
            None,
            "[1]",
        );
        assert_eq!(list, 400);
    }

    #[test]
    fn token_is_enforced() {
        let p = policy();
        let body = r#"{"action": "lookup"}"#;
        let (missing, _) = route(
            &p,
            None,
            Some("s3cret"),
            &Method::Post,
            "/v1/decide",
            None,
            body,
        );
        assert_eq!(missing, 401);
        let (wrong, _) = route(
            &p,
            None,
            Some("s3cret"),
            &Method::Post,
            "/v1/decide",
            Some("Bearer nope"),
            body,
        );
        assert_eq!(wrong, 401);
        let (ok, _) = route(
            &p,
            None,
            Some("s3cret"),
            &Method::Post,
            "/v1/decide",
            Some("Bearer s3cret"),
            body,
        );
        assert_eq!(ok, 200);
        let (health, _) = route(&p, None, Some("s3cret"), &Method::Get, "/healthz", None, "");
        assert_eq!(health, 200);
    }

    #[test]
    fn wrong_method_and_unknown_path() {
        let (wrong_method, _) = route(&policy(), None, None, &Method::Get, "/v1/decide", None, "");
        assert_eq!(wrong_method, 405);
        let (unknown, _) = route(&policy(), None, None, &Method::Get, "/nope", None, "");
        assert_eq!(unknown, 404);
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
