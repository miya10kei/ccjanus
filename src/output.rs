use crate::types::HookOutput;

/// Emit an allow decision (stdout JSON).
pub fn emit_allow() {
    let output = HookOutput {
        decision: Some("approve".to_string()),
        reason: None,
    };
    println!("{}", serde_json::to_string(&output).unwrap());
}

/// Emit a deny decision (stderr JSON, exit code 2).
pub fn emit_deny(reason: &str) -> ! {
    let output = HookOutput {
        decision: Some("block".to_string()),
        reason: Some(reason.to_string()),
    };
    eprintln!("{}", serde_json::to_string(&output).unwrap());
    std::process::exit(2);
}

/// Emit a fallthrough (silent exit 0).
pub fn emit_fallthrough(reason: &str, explain: bool) {
    if explain {
        eprintln!("[ccjanus] fallthrough: {reason}");
    }
}
