use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn example(relative: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Starts `tpt-policy serve` and waits until it accepts connections.
fn start_server(token: Option<&str>) -> (Child, String) {
    let addr = format!("127.0.0.1:{}", free_port());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tpt-policy"));
    cmd.args(["serve", &example("expense/expense.yaml"), "--listen", &addr])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_remove("TPT_POLICY_TOKEN");
    if let Some(t) = token {
        cmd.env("TPT_POLICY_TOKEN", t);
    }
    let child = cmd.spawn().expect("server starts");

    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(&addr).is_err() {
        assert!(Instant::now() < deadline, "server did not start on {addr}");
        sleep(Duration::from_millis(50));
    }
    (child, addr)
}

/// Sends one HTTP/1.1 request and returns (status code, body).
fn http(addr: &str, request: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status line");
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

fn post_evaluate(body: &str, auth: Option<&str>) -> String {
    let auth_header = auth
        .map(|a| format!("Authorization: {a}\r\n"))
        .unwrap_or_default();
    format!(
        "POST /v1/evaluate HTTP/1.1\r\nHost: localhost\r\n{auth_header}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn serve_evaluates_over_http() {
    let (mut child, addr) = start_server(None);

    let (status, health) = http(
        &addr,
        "GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(status, 200);
    assert_eq!(health, r#"{"status":"ok"}"#);

    let (status, body) = http(
        &addr,
        &post_evaluate(r#"{"expense":{"amount":6200}}"#, None),
    );
    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["decision"], "approval_required");
    assert_eq!(json["approvers"], serde_json::json!(["manager", "finance"]));

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn serve_enforces_token() {
    let (mut child, addr) = start_server(Some("s3cret"));

    let (status, _) = http(&addr, &post_evaluate(r#"{"expense":{"amount":1}}"#, None));
    assert_eq!(status, 401);

    let (status, _) = http(
        &addr,
        &post_evaluate(r#"{"expense":{"amount":1}}"#, Some("Bearer s3cret")),
    );
    assert_eq!(status, 200);

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn doctor_passes_on_this_install() {
    let out = Command::new(env!("CARGO_BIN_EXE_tpt-policy"))
        .arg("doctor")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("[ok] self-test"), "{text}");
    assert!(text.contains("[ok] working directory"), "{text}");
    assert_eq!(out.status.code(), Some(0), "{text}");
}
