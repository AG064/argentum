//! Synthetic offline interoperability controls, not an installed extension.
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            break;
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        let method = request["method"].as_str().unwrap_or_default();
        if mode == "legacy" && method == "server/discover" {
            println!(
                "{}",
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"legacy control"}})
            );
            io::stdout().flush().unwrap();
            continue;
        }
        let result = match method {
            "server/discover" => serde_json::to_value(rmcp::model::DiscoverResult::new(
                vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                serde_json::from_value(json!({"tools":{}})).unwrap(),
            ))
            .unwrap(),
            "initialize" => {
                json!({"protocolVersion":"2025-11-25","serverInfo":{"name":"offline-control","version":"1"},"capabilities":{"tools":{}}})
            }
            "tools/list" => {
                let schema = if mode == "bad-schema" {
                    json!({"type":"array"})
                } else {
                    json!({"type":"object","properties":{"message":{"type":"string"}},"required":["message"],"additionalProperties":false})
                };
                json!({"tools":[{"name":"echo","description":"Synthetic scoped echo","inputSchema":schema}]})
            }
            "tools/call" => {
                if mode == "slow" {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
                let text = if mode == "oversize" {
                    "x".repeat(70 * 1024)
                } else {
                    request["params"]["arguments"]["message"]
                        .as_str()
                        .unwrap_or("missing")
                        .to_owned()
                };
                json!({"content":[{"type":"text","text":text}]})
            }
            _ => {
                println!(
                    "{}",
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"unsupported control method"}})
                );
                io::stdout().flush().unwrap();
                continue;
            }
        };
        println!("{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
        io::stdout().flush().unwrap();
    }
}
