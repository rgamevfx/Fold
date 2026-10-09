#![cfg(target_os = "linux")]
use fold_app::session::Session;
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::desktop::DesktopClient;
use fold_project::{EditBatch, Mutation, Project};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn mcp_subprocess_uses_shared_schema_and_native_proposal_apply() {
    let id = DocumentId::new();
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                fold_motion::package::create(id).unwrap(),
            )],
        })
        .unwrap();
    let mut session = Session::new(project);
    let directory = tempfile::tempdir().unwrap();
    let path = session.enable_agent_transport(directory.path()).unwrap();
    let worker = std::thread::spawn(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fold-cli"))
            .arg("mcp")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut rpc_id = 0;
        let mut exchange = |method: &str, params: Value| {
            rpc_id += 1;
            writeln!(
                input,
                "{}",
                json!({"jsonrpc":"2.0","id":rpc_id,"method":method,"params":params})
            )
            .unwrap();
            input.flush().unwrap();
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            let response: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["id"], rpc_id);
            assert!(response.get("error").is_none(), "{response}");
            if method == "initialize" {
                writeln!(
                    input,
                    "{}",
                    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
                )
                .unwrap();
                input.flush().unwrap();
            }
            response["result"].clone()
        };
        assert_eq!(
            exchange(
                "initialize",
                json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}})
            )["protocolVersion"],
            "2025-11-25"
        );
        let schema = exchange("tools/list", json!({}))["tools"][0]["inputSchema"].clone();
        let mut tool = |command: Value| {
            let mut response = exchange(
                "tools/call",
                json!({"name":"fold_request","arguments":{"version":2,"command":command}}),
            );
            assert_eq!(response["isError"], false, "{response}");
            let handle = response["structuredContent"]["result"]["job"].clone();
            while response["structuredContent"]["result"]["state"] == "running" {
                response = exchange(
                    "tools/call",
                    json!({"name":"fold_request","arguments":{"version":2,"command":{"method":"job_status","handle":handle,"wait_ms":1000}}}),
                );
                assert_eq!(response["isError"], false, "{response}");
            }
            response["structuredContent"].clone()
        };
        let discovery = tool(json!({"method":"discover"}));
        assert_eq!(schema, discovery["result"]["request_schema"]);
        let guard = || json!({"epoch":discovery["epoch"],"request_id":ObjectId::new(),"base":discovery["result"]["revision"],"scope":{"document":id,"definition":null}});
        let proposal = tool(
            json!({"method":"propose","guard":guard(),"edit":{"kind":"patches","patches":[{"path":[{"kind":"field","value":"seed"}],"expected":{"kind":"present","value":0},"value":{"kind":"present","value":123}}]}}),
        );
        let handle = proposal["result"]["handle"].clone();
        let inspection = tool(json!({"method":"inspect_candidate","handle":handle}));
        let apply = json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspection["result"]["receipt"]});
        let applied = tool(apply.clone());
        assert_eq!(tool(apply), applied);
        drop(input);
        assert!(child.wait().unwrap().success());
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !worker.is_finished() {
        assert!(Instant::now() < deadline, "MCP subprocess did not finish");
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap();
    let current = session.snapshot().unwrap();
    assert_eq!(
        fold_motion::model::Construction::from_document(&current.state().documents[&id])
            .unwrap()
            .seed,
        123
    );
}
