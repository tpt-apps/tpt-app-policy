use std::path::PathBuf;
use std::process::{Command, Output};

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/transform")
        .join(relative)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-transform"))
        .args(args)
        .output()
        .expect("tpt-transform runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exits normally")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-transform-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn run_example(out: &std::path::Path) -> Output {
    run(&[
        "run",
        example("customers.csv").to_str().unwrap(),
        "--pipeline",
        example("customers.pipeline.yaml").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ])
}

#[test]
fn example_matches_the_golden_output() {
    let out = scratch("golden");
    let result = run_example(&out);
    assert_eq!(code(&result), 10, "one record is rejected");
    let transformed = lines(&out.join("transformed.csv"));
    let expected = lines(&example("expected/transformed.csv"));
    assert_eq!(transformed, expected);
    let rejected = lines(&out.join("rejected.jsonl"));
    let expected = lines(&example("expected/rejected.jsonl"));
    assert_eq!(rejected, expected);
}

#[test]
fn summary_counts_every_record_once() {
    let out = scratch("summary");
    run_example(&out);
    let text = std::fs::read_to_string(out.join("summary.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["counts"]["read"], 5);
    assert_eq!(v["counts"]["transformed"], 3);
    assert_eq!(v["counts"]["filtered"], 1);
    assert_eq!(v["counts"]["rejected"], 1);
    assert_eq!(v["pipeline"]["name"], "customer-clean");
    assert_eq!(v["pipeline"]["steps"], 9);
}

#[test]
fn output_is_byte_identical_across_runs() {
    let first = scratch("repeat-a");
    let second = scratch("repeat-b");
    run_example(&first);
    run_example(&second);
    for name in ["transformed.csv", "rejected.jsonl", "summary.json"] {
        assert_eq!(
            std::fs::read(first.join(name)).unwrap(),
            std::fs::read(second.join(name)).unwrap(),
            "{name} differs between runs"
        );
    }
}

#[test]
fn clean_input_exits_0() {
    let out = scratch("clean");
    let input = scratch("clean-input");
    std::fs::create_dir_all(&input).unwrap();
    let file = input.join("people.csv");
    std::fs::write(&file, "first_name,last_name,email,country,amount,fee,status\nAda,Lovelace,a@x.com,NZ,1,1,active\n").unwrap();
    let result = run(&[
        "run",
        file.to_str().unwrap(),
        "--pipeline",
        example("customers.pipeline.yaml").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 0);
    assert!(String::from_utf8_lossy(&result.stdout).contains("transformed 1 of 1 records"));
}

#[test]
fn json_array_can_be_written_as_jsonl() {
    let out = scratch("to-jsonl");
    let input = scratch("to-jsonl-input");
    std::fs::create_dir_all(&input).unwrap();
    let file = input.join("items.json");
    std::fs::write(&file, r#"[{"name": " Ada "}, {"name": "Bo"}]"#).unwrap();
    let pipeline = input.join("p.yaml");
    std::fs::write(&pipeline, "pipeline:\n  - trim:\n      fields: [\"*\"]\n").unwrap();
    let result = run(&[
        "run",
        file.to_str().unwrap(),
        "--pipeline",
        pipeline.to_str().unwrap(),
        "--to",
        "jsonl",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 0);
    let text = std::fs::read_to_string(out.join("transformed.jsonl")).unwrap();
    assert_eq!(text, "{\"name\":\"Ada\"}\n{\"name\":\"Bo\"}\n");
}

#[test]
fn invalid_pipeline_exits_2_and_names_the_problem() {
    let input = scratch("bad-pipeline");
    std::fs::create_dir_all(&input).unwrap();
    let pipeline = input.join("p.yaml");
    std::fs::write(&pipeline, "pipeline:\n  - shout:\n      fields: [a]\n").unwrap();
    let result = run(&["check", "--pipeline", pipeline.to_str().unwrap()]);
    assert_eq!(code(&result), 2);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("invalid pipeline"), "{stderr}");
    assert!(stderr.contains("shout"), "{stderr}");
}

#[test]
fn check_accepts_the_example_pipeline() {
    let result = run(&[
        "check",
        "--pipeline",
        example("customers.pipeline.yaml").to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 0);
    assert!(String::from_utf8_lossy(&result.stdout).contains("customer-clean (9 steps)"));
}

#[test]
fn malformed_json_array_exits_3() {
    let input = scratch("bad-array");
    std::fs::create_dir_all(&input).unwrap();
    let file = input.join("broken.json");
    std::fs::write(&file, "[{\"name\": ").unwrap();
    let pipeline = example("customers.pipeline.yaml");
    let out = scratch("bad-array-out");
    let result = run(&[
        "run",
        file.to_str().unwrap(),
        "--pipeline",
        pipeline.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 3);
}

#[test]
fn doctor_passes() {
    let result = run(&["doctor"]);
    assert_eq!(code(&result), 0);
    assert!(String::from_utf8_lossy(&result.stdout).contains("all checks passed"));
}

/// Read a text file with line endings made LF, so a checkout with CRLF still matches.
fn lines(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .expect("file is readable")
        .replace("\r\n", "\n")
}
