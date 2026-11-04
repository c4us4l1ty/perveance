//! Experimental, opt-in OpenRouter adapter. It never executes generated code.
//! Responses are untrusted and restricted to edits on newly added raster layers.

use crate::{EditProposal, Operation, Project, PROPOSAL_VERSION};
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
use std::time::Duration;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MAX_PROMPT_BYTES: usize = 4000;
const MAX_RESPONSE_BYTES: u64 = 256 * 1024;
const MAX_AI_OPERATIONS: usize = 256;
const MAX_OUTPUT_TOKENS: u32 = 2048;

/// The only project data sent to the provider are dimensions and layer count.
/// The revision stays local and is attached to the resulting proposal.
#[derive(Clone)]
pub struct TaskContext {
    width: u32,
    height: u32,
    layers: usize,
    revision: String,
}

impl TaskContext {
    pub fn from_project(project: &Project) -> Result<Self, String> {
        project.validate()?;
        if project.layers.len() >= 32 { return Err("AI needs room for a new layer (limit 32)".into()); }
        Ok(Self { width: project.width, height: project.height, layers: project.layers.len(), revision: project.revision()? })
    }
}

/// Validate without logging or including the supplied key/prompt in any error.
pub fn validate_request(model: &str, key: &str, prompt: &str) -> Result<(), String> {
    if model.is_empty() || model.len() > 128 || !model.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._:-".contains(&b)) {
        return Err("invalid model ID".into());
    }
    if key.is_empty() || key.len() > 512 || !key.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err("invalid API key format".into());
    }
    if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT_BYTES {
        return Err(format!("prompt must contain 1 to {MAX_PROMPT_BYTES} bytes"));
    }
    Ok(())
}

/// Send one request to the fixed HTTPS OpenRouter endpoint. No arbitrary endpoint,
/// proxy environment variables, redirects, generated code, or filesystem tools.
pub fn propose_openrouter(context: &TaskContext, model: &str, key: &str, prompt: &str) -> Result<EditProposal, String> {
    request(context, model, key, prompt, OPENROUTER_URL, true)
}

fn request(context: &TaskContext, model: &str, key: &str, prompt: &str, url: &str, https_only: bool) -> Result<EditProposal, String> {
    validate_request(model, key, prompt)?;
    let system = format!(
        "You create pixel art using JSON operations only. Output exactly an object with an operations array, no prose or code. \
        Canvas is {} by {} pixels. Existing layers are indexed 0 through {}; you must not edit them. \
        Add new layers with {{\"op\":\"add_layer\"}}; the first new layer has index {}. \
        Only add_layer, rename_layer, pixel, line, rectangle, rectangle_outline, ellipse, fill are allowed. \
        Rename and paint only new layers. Maximum 256 operations and 32 total layers. Coordinates are zero-based and must fit. \
        Color is a four-byte [r,g,b,a] array. Examples: \
        {{\"op\":\"rename_layer\",\"layer\":{},\"name\":\"Art\"}}, \
        {{\"op\":\"pixel\",\"layer\":{},\"x\":0,\"y\":0,\"color\":[255,0,0,255]}}, \
        {{\"op\":\"line\",\"layer\":{},\"from\":[0,0],\"to\":[0,0],\"color\":[255,0,0,255]}}, \
        {{\"op\":\"rectangle\",\"layer\":{},\"x\":0,\"y\":0,\"width\":1,\"height\":1,\"color\":[255,0,0,255]}}, \
        {{\"op\":\"rectangle_outline\",\"layer\":{},\"x\":0,\"y\":0,\"width\":1,\"height\":1,\"color\":[255,0,0,255]}}, \
        {{\"op\":\"ellipse\",\"layer\":{},\"x\":0,\"y\":0,\"width\":1,\"height\":1,\"filled\":true,\"color\":[255,0,0,255]}}, \
        {{\"op\":\"fill\",\"layer\":{},\"x\":0,\"y\":0,\"color\":[255,0,0,255]}}. \
        You do not see existing pixels or layer names.",
        context.width, context.height, context.layers - 1, context.layers,
        context.layers, context.layers, context.layers, context.layers, context.layers, context.layers, context.layers,
    );
    let body = json!({
        "model": model, "messages": [{"role":"system", "content":system}, {"role":"user", "content":prompt}],
        "response_format":{"type":"json_object"}, "max_tokens":MAX_OUTPUT_TOKENS, "stream":false, "n":1
    });
    let body = serde_json::to_vec(&body).map_err(|_| "could not build provider request")?;
    let agent = ureq::AgentBuilder::new()
        .try_proxy_from_env(false).https_only(https_only).redirects(0)
        .timeout(Duration::from_secs(60)).build();
    let response = match agent.post(url)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_bytes(&body) {
        Ok(response) => response,
        Err(ureq::Error::Status(status, _)) => return Err(format!("OpenRouter returned HTTP {status}")),
        Err(_) => return Err("OpenRouter request failed or timed out".into()),
    };
    if !(200..300).contains(&response.status()) { return Err(format!("OpenRouter returned HTTP {}", response.status())); }
    let mut bytes = Vec::new();
    response.into_reader().take(MAX_RESPONSE_BYTES + 1).read_to_end(&mut bytes)
        .map_err(|_| "could not read provider response")?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES { return Err("provider response exceeds size limit".into()); }
    parse_response(context, &bytes)
}

#[derive(Deserialize)]
struct ProviderResponse { choices: Vec<ProviderChoice> }
#[derive(Deserialize)]
struct ProviderChoice { finish_reason: Option<String>, message: ProviderMessage }
#[derive(Deserialize)]
struct ProviderMessage { content: Option<String> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedOperations { operations: Vec<Operation> }

fn parse_response(context: &TaskContext, bytes: &[u8]) -> Result<EditProposal, String> {
    let response: ProviderResponse = serde_json::from_slice(bytes).map_err(|_| "invalid provider response")?;
    if response.choices.len() != 1 { return Err("provider returned an unexpected number of choices".into()); }
    let choice = response.choices.into_iter().next().expect("one choice");
    if choice.finish_reason.as_deref() != Some("stop") { return Err("provider response was incomplete or refused".into()); }
    let content = choice.message.content.ok_or("provider returned no text")?;
    let generated: GeneratedOperations = serde_json::from_str(&content).map_err(|_| "provider did not return valid typed operations")?;
    validate_generated(&generated.operations, context.layers)?;
    Ok(EditProposal { version: PROPOSAL_VERSION, base_revision: context.revision.clone(), operations: generated.operations })
}

fn validate_generated(operations: &[Operation], initial_layers: usize) -> Result<(), String> {
    if operations.len() > MAX_AI_OPERATIONS { return Err("provider returned too many operations".into()); }
    let mut layers = initial_layers;
    for op in operations {
        match op {
            Operation::AddLayer if layers < 32 => layers += 1,
            Operation::RenameLayer { layer, .. } | Operation::Pixel { layer, .. } |
            Operation::Line { layer, .. } | Operation::Rectangle { layer, .. } |
            Operation::RectangleOutline { layer, .. } | Operation::Ellipse { layer, .. } |
            Operation::Fill { layer, .. } if *layer >= initial_layers && *layer < layers => {}
            _ => return Err("provider may only create and paint new layers".into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn context() -> TaskContext { TaskContext::from_project(&Project::new(2, 2).unwrap()).unwrap() }
    fn response(content: &str, finish: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"choices":[{"finish_reason":finish,"message":{"content":content}}]})).unwrap()
    }
    fn serve_once(status: &str, body: Vec<u8>, extra_header: &str) -> (String, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        let extra_header = extra_header.to_owned();
        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            let header_end = loop {
                let mut buf = [0; 1024];
                let n = socket.read(&mut buf).unwrap();
                assert_ne!(n, 0);
                request.extend_from_slice(&buf[..n]);
                if let Some(pos) = request.windows(4).position(|w| w == b"\r\n\r\n") { break pos + 4; }
                assert!(request.len() < 16384);
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let length: usize = headers.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().unwrap())
            }).unwrap();
            while request.len() < header_end + length {
                let mut buf = [0; 1024];
                let n = socket.read(&mut buf).unwrap();
                assert_ne!(n, 0);
                request.extend_from_slice(&buf[..n]);
            }
            write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_header}Connection: close\r\n\r\n", body.len()).unwrap();
            let _ = socket.write_all(&body);
            request
        });
        (format!("http://{address}/api"), handle)
    }

    #[test]
    fn fake_provider_round_trip_uses_restricted_data_and_typed_edits() {
        let content = r#"{"operations":[{"op":"add_layer"},{"op":"pixel","layer":1,"x":1,"y":1,"color":[1,2,3,255]}]}"#;
        let (url, server) = serve_once("200 OK", response(content, "stop"), "");
        let mut project = Project::new(2, 2).unwrap();
        project.rename_layer(0, "private-layer-sentinel").unwrap();
        project.set_pixel(0, 0, 0, [43, 67, 89, 135]).unwrap();
        let original_layer = project.layers[0].clone();
        let context = TaskContext::from_project(&project).unwrap();
        let proposal = request(&context, "test/model", "fake-key", "paint one pixel", &url, false).unwrap();
        let received = server.join().unwrap();
        let header_end = received.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let headers = String::from_utf8_lossy(&received[..header_end]).to_ascii_lowercase();
        assert!(headers.contains("authorization: bearer fake-key"));
        let payload: serde_json::Value = serde_json::from_slice(&received[header_end..]).unwrap();
        assert_eq!(payload["model"], "test/model");
        assert_eq!(payload["max_tokens"], MAX_OUTPUT_TOKENS);
        assert_eq!(payload["messages"][1]["content"], "paint one pixel");
        let body = String::from_utf8_lossy(&received[header_end..]);
        assert!(!body.contains("fake-key"));
        assert!(!body.contains("private-layer-sentinel"));
        assert!(!body.contains("[43,67,89,135]"));
        assert!(!body.contains(&context.revision));
        project.apply_proposal(&proposal).unwrap();
        assert_eq!(project.layers[0], original_layer);
        assert_eq!(project.layers[1].pixels[3], [1, 2, 3, 255]);
    }

    #[test]
    fn rejects_invalid_provider_output_and_edits_to_existing_layers() {
        let ctx = context();
        assert!(parse_response(&ctx, &response(r#"{"operations":[{"op":"pixel","layer":0,"x":0,"y":0,"color":[0,0,0,0]}]}"#, "stop")).is_err());
        assert!(parse_response(&ctx, &response(r#"{"operations":[{"op":"remove_layer","layer":0}]}"#, "stop")).is_err());
        assert!(parse_response(&ctx, &response(r#"{"operations":[{"op":"add_layer"},{"op":"clear_layer","layer":1}]}"#, "stop")).is_err());
        assert!(parse_response(&ctx, &response(r#"{"operations":[],"code":"hello"}"#, "stop")).is_err());
        assert!(parse_response(&ctx, &response(r#"{"operations":[]}"#, "length")).is_err());
        assert!(parse_response(&ctx, b"malformed").is_err());
        assert!(validate_generated(&vec![Operation::AddLayer; MAX_AI_OPERATIONS + 1], 1).is_err());
        assert!(validate_generated(&[Operation::AddLayer], 32).is_err());
    }

    #[test]
    fn http_errors_redirects_and_response_limits_do_not_leak_bodies() {
        let ctx = context();
        for (status, body, headers) in [
            ("429 Too Many Requests", b"fake-secret-in-response".to_vec(), ""),
            ("302 Found", b"fake-secret-in-response".to_vec(), "Location: http://127.0.0.1:1/\r\n"),
            ("307 Temporary Redirect", b"fake-secret-in-response".to_vec(), "Location: http://127.0.0.1:1/\r\n"),
            ("200 OK", vec![b'x'; MAX_RESPONSE_BYTES as usize + 1], ""),
        ] {
            let (url, server) = serve_once(status, body, headers);
            let error = request(&ctx, "test/model", "fake-key", "art", &url, false).unwrap_err();
            server.join().unwrap();
            assert!(!error.contains("fake-key"));
            assert!(!error.contains("fake-secret-in-response"));
            assert!(error.contains("HTTP") || error.contains("size limit"), "{error}");
        }
    }

    #[test]
    fn validates_inputs_without_returning_secret_or_prompt() {
        assert!(validate_request("test/model", "fake-key", "art").is_ok());
        assert!(validate_request("bad\nmodel", "fake-key", "art").is_err());
        assert!(validate_request("test/model", "fake key", "art").is_err());
        assert!(validate_request("test/model", "fake-key", &"x".repeat(MAX_PROMPT_BYTES + 1)).is_err());
        assert!(TaskContext::from_project(&Project { layers: vec![], ..Project::new(1, 1).unwrap() }).is_err());
    }
}
