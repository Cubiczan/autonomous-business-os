use abos_governance_core::{
    classify_action, inspect_prompt_injection, sign_audit_event, verify_signed_audit_event,
    AuditEvent,
};
use serde_json::Value;
use std::env;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        std::process::exit(2);
    };

    match command.as_str() {
        "classify-action" => {
            let action_type = required_arg(args.next(), "action type")?;
            let payload: Value = args
                .next()
                .map(|raw| serde_json::from_str(&raw))
                .transpose()?
                .unwrap_or(Value::Object(Default::default()));
            println!(
                "{}",
                serde_json::to_string_pretty(&classify_action(&action_type, &payload))?
            );
        }
        "inspect-text" => {
            let source = required_arg(args.next(), "source")?;
            let text = required_arg(args.next(), "text")?;
            println!(
                "{}",
                serde_json::to_string_pretty(&inspect_prompt_injection(&text, &source))?
            );
        }
        "sign-event" => {
            let event_path = required_arg(args.next(), "event json path")?;
            let key_id = args.next().unwrap_or_else(|| "local".to_string());
            let secret_env = args
                .next()
                .unwrap_or_else(|| "ABOS_LEDGER_SIGNING_KEY".to_string());
            let secret = env::var(&secret_env)
                .map_err(|_| format!("missing ledger signing key env var: {secret_env}"))?;
            let event: AuditEvent = serde_json::from_str(&fs::read_to_string(event_path)?)?;
            let entry = sign_audit_event(&event, &secret, &key_id)?;
            println!("{}", serde_json::to_string_pretty(&entry)?);
        }
        "verify-event" => {
            let event_path = required_arg(args.next(), "event json path")?;
            let entry_path = required_arg(args.next(), "signed ledger entry json path")?;
            let secret_env = args
                .next()
                .unwrap_or_else(|| "ABOS_LEDGER_SIGNING_KEY".to_string());
            let secret = env::var(&secret_env)
                .map_err(|_| format!("missing ledger signing key env var: {secret_env}"))?;
            let event: AuditEvent = serde_json::from_str(&fs::read_to_string(event_path)?)?;
            let entry = serde_json::from_str(&fs::read_to_string(entry_path)?)?;
            let ok = verify_signed_audit_event(&event, &entry, &secret)?;
            println!("{}", serde_json::json!({"ok": ok}));
            std::process::exit(if ok { 0 } else { 1 });
        }
        _ => {
            print_help();
            std::process::exit(2);
        }
    }
    Ok(())
}

fn required_arg(value: Option<String>, label: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("missing {label}"))
}

fn print_help() {
    eprintln!(
        "Usage:
  abos-governance-core classify-action <action-type> [payload-json]
  abos-governance-core inspect-text <source> <text>
  abos-governance-core sign-event <event.json> [key-id] [secret-env]
  abos-governance-core verify-event <event.json> <entry.json> [secret-env]"
    );
}
