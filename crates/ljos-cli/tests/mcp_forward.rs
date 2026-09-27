//! A stale `ljos-mcp` forwards a tool call to the installed binary through
//! `mcp_forward`; this drives that exchange against the built binary.

use std::path::Path;

#[test]
fn a_forwarded_call_is_answered_by_the_fresh_server() {
    let runtime = tempfile::tempdir().unwrap();
    let cards = tempfile::tempdir().unwrap();
    std::fs::write(cards.path().join("USER.md"), "frozen by the test\n").unwrap();
    // Safety: this test binary runs one test; the child inherits both.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", runtime.path());
        std::env::set_var("LJOS_CARDS_DIR", cards.path());
    }
    let params = serde_json::json!({"name": "ljos_cards", "arguments": {}});
    let init = serde_json::json!({"protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "acme-cli", "version": "1"}});
    let answer = ljos_cli::mcp_forward(
        Path::new(env!("CARGO_BIN_EXE_ljos-mcp")),
        "LJOS_MCP_FORWARDED",
        Some(init),
        params,
    )
    .unwrap();
    assert_eq!(answer["id"], 1, "{answer}");
    assert!(
        answer["result"].to_string().contains("frozen by the test"),
        "{answer}"
    );
}
