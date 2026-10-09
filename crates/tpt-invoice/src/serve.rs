//! `tpt-invoice serve`: HTTP interface for checking one invoice at a time.
//!
//! Endpoints:
//!
//! - `POST /v1/validate`: body is one invoice as a JSON object; response is the
//!   result JSON (verdict, checks, evaluation).
//! - `GET /healthz`: `{"status": "ok"}`.
//!
//! If a token is set, `POST /v1/validate` requires `Authorization: Bearer <token>`.
//! Requests are handled one at a time, so the ledger needs no locking. Routing
//! lives in [`route`], which takes no sockets, so it can be tested directly.

use std::io::Read;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};
use tpt_document::{DocFormat, Parsed};
use tpt_invoice::{check_invoice, duplicate_key, Ledger, Suppliers, Verdict};
use tpt_policy_core::Policy;
use tpt_schema::Schema;

/// Largest request body accepted, in bytes.
pub const MAX_BODY_BYTES: u64 = 1024 * 1024;

/// What is needed to check an invoice. The ledger is updated as invoices are accepted.
pub struct Service {
    pub schema: Schema,
    pub policy: Option<Policy>,
    pub suppliers: Option<Suppliers>,
    pub ledger: Option<Ledger>,
    pub token: Option<String>,
}

/// Serve invoice checks on `addr`. Blocks until stopped.
pub fn serve(mut service: Service, addr: &str) -> Result<(), String> {
    let server = Server::http(addr).map_err(|e| e.to_string())?;
    println!(
        "listening on http://{addr} (schema {} version {}, policy {}, ledger {}, auth {})",
        service.schema.name,
        service.schema.version,
        service
            .policy
            .as_ref()
            .map_or("off".to_string(), |p| format!(
                "{} version {}",
                p.name, p.version
            )),
        if service.ledger.is_some() {
            "on"
        } else {
            "off"
        },
        if service.token.is_some() {
            "required"
        } else {
            "off"
        }
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
                    "send the invoice as UTF-8 JSON",
                ),
            ),
            Ok(n) if n as u64 > MAX_BODY_BYTES => (
                413,
                error_body(
                    "body too large",
                    &format!("the body is over {MAX_BODY_BYTES} bytes"),
                    "send one invoice per request",
                ),
            ),
            Ok(_) => route(&mut service, &method, &path, auth.as_deref(), &body),
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
    service: &mut Service,
    method: &Method,
    path: &str,
    auth: Option<&str>,
    body: &str,
) -> (u16, String) {
    match (method, path) {
        (Method::Get, "/healthz") => (200, json!({"status": "ok"}).to_string()),
        (Method::Post, "/v1/validate") => {
            if let Some(expected) = service.token.as_deref() {
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
            validate(service, body)
        }
        (_, "/healthz") | (_, "/v1/validate") => (
            405,
            error_body(
                "method not allowed",
                &format!("{method} is not supported on {path}"),
                "use GET for /healthz and POST for /v1/validate",
            ),
        ),
        _ => (
            404,
            error_body(
                "not found",
                &format!("there is no endpoint at {path}"),
                "use POST /v1/validate or GET /healthz",
            ),
        ),
    }
}

/// Check one invoice in the body. An accepted invoice is added to the ledger.
fn validate(service: &mut Service, body: &str) -> (u16, String) {
    let value = match serde_json::from_str::<Value>(body) {
        Ok(value @ Value::Object(_)) => value,
        Ok(_) => {
            return (
                400,
                error_body(
                    "invalid request",
                    "the body is JSON but not an object",
                    "send one invoice as a JSON object",
                ),
            )
        }
        Err(e) => {
            return (
                400,
                error_body(
                    "invalid JSON",
                    &e.to_string(),
                    "send one invoice as a JSON object in the body",
                ),
            )
        }
    };
    let parsed = Parsed::Ok {
        format: DocFormat::Json,
        value: value.clone(),
    };
    let result = check_invoice(
        &service.schema,
        service.policy.as_ref(),
        service.suppliers.as_ref(),
        service.ledger.as_ref(),
        &parsed,
    );

    if let (Some(ledger), Some(key)) = (service.ledger.as_mut(), duplicate_key(&value)) {
        if result.verdict != Verdict::Reject {
            if let Err(e) = ledger.record(&key) {
                return (
                    500,
                    error_body(
                        "ledger not written",
                        &e.to_string(),
                        "check the ledger file is writable, then send the invoice again",
                    ),
                );
            }
        }
    }

    (
        200,
        serde_json::to_string_pretty(&result).expect("result is always serialisable"),
    )
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
    use tpt_schema::parse_schema;

    const SCHEMA: &str = "schema: t\nfields:\n  invoice_number:\n    type: string\n    required: true\n  supplier.tax_id:\n    type: string\n    required: true\n  total:\n    type: number\n    required: true\n";

    fn service(token: Option<&str>) -> Service {
        Service {
            schema: parse_schema(SCHEMA).unwrap(),
            policy: None,
            suppliers: None,
            ledger: None,
            token: token.map(str::to_string),
        }
    }

    const GOOD: &str = r#"{"invoice_number": "A-1", "supplier": {"tax_id": "T1"}, "total": 10, "subtotal": 10, "tax": 0, "lines": [{"quantity": 1, "unit_price": 10, "amount": 10}]}"#;

    #[test]
    fn health_check() {
        let (status, body) = route(&mut service(None), &Method::Get, "/healthz", None, "");
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"status":"ok"}"#);
    }

    #[test]
    fn valid_invoice_returns_a_pass_verdict() {
        let (status, body) = route(
            &mut service(None),
            &Method::Post,
            "/v1/validate",
            None,
            GOOD,
        );
        assert_eq!(status, 200);
        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["verdict"], "PASS");
    }

    #[test]
    fn invoice_with_a_bad_total_is_rejected_not_an_http_error() {
        let bad = GOOD.replace("\"total\": 10", "\"total\": 99");
        let (status, body) = route(
            &mut service(None),
            &Method::Post,
            "/v1/validate",
            None,
            &bad,
        );
        assert_eq!(
            status, 200,
            "a rejected invoice is a result, not a request error"
        );
        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["verdict"], "REJECT");
    }

    #[test]
    fn schema_failure_is_reported_in_the_result() {
        let (status, body) = route(
            &mut service(None),
            &Method::Post,
            "/v1/validate",
            None,
            r#"{"total": 1}"#,
        );
        assert_eq!(status, 200);
        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["verdict"], "REJECT");
        assert!(json["schema_errors"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn bad_json_and_non_object_are_400() {
        let (bad, _) = route(
            &mut service(None),
            &Method::Post,
            "/v1/validate",
            None,
            "{nope",
        );
        assert_eq!(bad, 400);
        let (list, _) = route(
            &mut service(None),
            &Method::Post,
            "/v1/validate",
            None,
            "[1]",
        );
        assert_eq!(list, 400);
    }

    #[test]
    fn token_is_enforced() {
        let mut s = service(Some("s3cret"));
        let (missing, _) = route(&mut s, &Method::Post, "/v1/validate", None, GOOD);
        assert_eq!(missing, 401);
        let (wrong, _) = route(
            &mut s,
            &Method::Post,
            "/v1/validate",
            Some("Bearer nope"),
            GOOD,
        );
        assert_eq!(wrong, 401);
        let (ok, _) = route(
            &mut s,
            &Method::Post,
            "/v1/validate",
            Some("Bearer s3cret"),
            GOOD,
        );
        assert_eq!(ok, 200);
        let (health, _) = route(&mut s, &Method::Get, "/healthz", None, "");
        assert_eq!(health, 200, "health check needs no token");
    }

    #[test]
    fn wrong_method_and_unknown_path() {
        let (wrong_method, _) = route(&mut service(None), &Method::Get, "/v1/validate", None, "");
        assert_eq!(wrong_method, 405);
        let (unknown, _) = route(&mut service(None), &Method::Get, "/nope", None, "");
        assert_eq!(unknown, 404);
    }

    #[test]
    fn accepted_invoices_join_the_ledger_and_repeats_are_rejected() {
        let path =
            std::env::temp_dir().join(format!("tpt-invoice-serve-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut s = service(None);
        s.ledger = Some(Ledger::load(&path).unwrap());

        let (_, first) = route(&mut s, &Method::Post, "/v1/validate", None, GOOD);
        assert_eq!(
            serde_json::from_str::<Value>(&first).unwrap()["verdict"],
            "PASS"
        );
        let (_, second) = route(&mut s, &Method::Post, "/v1/validate", None, GOOD);
        assert_eq!(
            serde_json::from_str::<Value>(&second).unwrap()["verdict"],
            "REJECT"
        );

        let saved = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            saved.lines().count(),
            1,
            "the accepted invoice is written once"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
