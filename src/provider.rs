//! Experimental, opt-in OpenRouter adapter. It never executes generated code.
//! Responses are untrusted and restricted to edits on newly added raster layers.

use crate::{EditProposal, Operation, Project, PROPOSAL_VERSION};
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
use std::time::Duration;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const MAX_PROMPT_BYTES: usize = 4000;
pub const MAX_API_KEY_BYTES: usize = 512;
pub const MAX_COMPOSITE_CONTEXT_PIXELS: u32 = 1024;
const MAX_RESPONSE_BYTES: u64 = 256 * 1024;
const MAX_AI_OPERATIONS: usize = 256;
const MAX_OUTPUT_TOKENS: u32 = 2048;

/// Dimensions/layer count are always sent; visible composite pixels are optional.
/// Hidden layers, names and the revision are not sent. Revision stays local.
#[derive(Clone)]
pub struct TaskContext {
    width: u32,
    height: u32,
    layers: usize,
    revision: String,
    composite_rows: Option<Vec<String>>,
}

impl TaskContext {
    pub fn from_project(project: &Project) -> Result<Self, String> {
        project.validate()?;
        if project.layers.len() >= 32 { return Err("AI needs room for a new layer (limit 32)".into()); }
        Ok(Self { width: project.width, height: project.height, layers: project.layers.len(), revision: project.revision()?, composite_rows: None })
    }

    /// Explicitly opt in to sending a bounded visible composite, encoded as
    /// rows of hexadecimal straight-alpha RGBA bytes. No hidden-layer data.
    pub fn from_project_with_composite(project: &Project) -> Result<Self, String> {
        let mut context = Self::from_project(project)?;
        if project.width * project.height > MAX_COMPOSITE_CONTEXT_PIXELS {
            return Err(format!("visible composite context is limited to {MAX_COMPOSITE_CONTEXT_PIXELS} pixels"));
        }
        let rgba = project.rgba()?;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut rows = Vec::with_capacity(project.height as usize);
        for row in rgba.chunks_exact(project.width as usize * 4) {
            let mut text = String::with_capacity(row.len() * 2);
            for byte in row {
                text.push(HEX[(byte >> 4) as usize] as char);
                text.push(HEX[(byte & 15) as usize] as char);
            }
            rows.push(text);
        }
        context.composite_rows = Some(rows);
        Ok(context)
    }
}

/// Validate without logging or including the supplied key/prompt in any error.
pub fn validate_request(model: &str, key: &str, prompt: &str) -> Result<(), String> {
    if model.is_empty() || model.len() > 128 || !model.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._:-".contains(&b)) {
        return Err("invalid model ID".into());
    }
    if key.is_empty() || key.len() > MAX_API_KEY_BYTES || !key.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
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
    let mut system = format!(
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
        {{\"op\":\"fill\",\"layer\":{},\"x\":0,\"y\":0,\"color\":[255,0,0,255]}}. ",
        context.width, context.height, context.layers - 1, context.layers,
        context.layers, context.layers, context.layers, context.layers, context.layers, context.layers, context.layers,
    );
    if let Some(rows) = &context.composite_rows {
        system.push_str("The user opted to share the visible composite. Rows below are top to bottom; each row contains left-to-right RGBA pixels, eight hexadecimal characters per pixel. Transparent composite pixels are 00000000. Existing layer names and hidden layers are not provided.\n");
        for row in rows { system.push_str(row); system.push('\n'); }
    } else {
        system.push_str("You do not see existing pixels or layer names.");
    }
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

/// Apply only provider-safe, revision-bound operations. Even a forged proposal
/// cannot modify old layers; normal project validation is also transactional.
pub fn apply_generated(project: &mut Project, proposal: &EditProposal) -> Result<(), String> {
    project.validate()?;
    validate_generated(&proposal.operations, project.layers.len())?;
    project.apply_proposal(proposal)
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
    fn visible_composite_opt_in_sends_only_composited_rgba() {
        let content = r#"{"operations":[{"op":"add_layer"},{"op":"pixel","layer":2,"x":1,"y":1,"color":[1,2,3,255]}]}"#;
        let (url, server) = serve_once("200 OK", response(content, "stop"), "");
        let mut project = Project::new(2, 2).unwrap();
        project.rename_layer(0, "private-name-sentinel").unwrap();
        project.set_pixel(0, 0, 0, [43, 67, 89, 135]).unwrap();
        project.set_pixel(0, 1, 0, [14, 15, 16, 0]).unwrap();
        project.add_layer().unwrap();
        project.rename_layer(1, "hidden-name-sentinel").unwrap();
        project.set_pixel(1, 0, 0, [222, 173, 190, 239]).unwrap();
        project.layers[1].visible = false;
        let ctx = TaskContext::from_project_with_composite(&project).unwrap();
        assert_eq!(ctx.composite_rows.as_ref().unwrap(), &["2b43598700000000", "0000000000000000"]);
        let proposal = request(&ctx, "test/model", "fake-key", "draw a star", &url, false).unwrap();
        let received = server.join().unwrap();
        let header_end = received.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let payload: serde_json::Value = serde_json::from_slice(&received[header_end..]).unwrap();
        let system = payload["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains("\n2b43598700000000\n0000000000000000\n"));
        assert!(!system.contains("private-name-sentinel"));
        assert!(!system.contains("hidden-name-sentinel"));
        assert!(!system.contains("deadbeef"));
        assert!(!system.contains(&project.revision().unwrap()));
        assert!(!system.contains("fake-key"));
        assert_eq!(payload["messages"][1]["content"], "draw a star");
        let original = project.layers.clone();
        apply_generated(&mut project, &proposal).unwrap();
        assert_eq!(&project.layers[..2], original.as_slice());
        assert_eq!(project.sample_layer(2, 1, 1).unwrap(), [1,2,3,255]);
    }

    #[test]
    fn visible_context_is_explicit_and_bounded_by_pixel_count() {
        let max = Project::new(32, 32).unwrap();
        let normal = TaskContext::from_project(&max).unwrap();
        assert!(normal.composite_rows.is_none());
        let included = TaskContext::from_project_with_composite(&max).unwrap();
        assert_eq!(included.composite_rows.as_ref().unwrap().len(), 32);
        assert!(included.composite_rows.as_ref().unwrap().iter().all(|row| row.len() == 32 * 8));
        assert_eq!(included.revision, normal.revision);
        let large = Project::new(33, 32).unwrap();
        assert!(TaskContext::from_project(&large).is_ok());
        assert!(TaskContext::from_project_with_composite(&large).err().unwrap().contains("1024"));
        let mut invalid = max;
        invalid.layers[0].pixels.pop();
        assert!(TaskContext::from_project_with_composite(&invalid).is_err());
    }

    #[test]
    fn provider_cannot_recolor_old_or_new_layers() {
        assert!(validate_generated(&[Operation::ReplaceColor { layer: 0, from: [0;4], to: [1;4] }], 1).is_err());
        assert!(validate_generated(&[Operation::AddLayer, Operation::ReplaceColor { layer: 1, from: [0;4], to: [1;4] }], 1).is_err());
    }

    #[test]
    fn provider_cannot_use_unlisted_brush_operations() {
        let brush = Operation::BrushLine { layer: 1, from: [0,0], to: [1,1], size: 3, color: [1;4] };
        assert!(validate_generated(&[Operation::AddLayer, brush], 1).is_err());
    }

    #[test]
    fn provider_cannot_resize_canvas() {
        assert!(validate_generated(&[Operation::ResizeCanvas { width: 1, height: 1, clip: true }], 1).is_err());
        assert!(validate_generated(&[Operation::AddLayer, Operation::ResizeCanvas { width: 3, height: 3, clip: false }], 1).is_err());
    }

    #[test]
    fn provider_cannot_translate_existing_or_new_layers() {
        assert!(validate_generated(&[Operation::TranslateLayer { layer: 0, dx: 1, dy: 0, clip: true }], 1).is_err());
        assert!(validate_generated(&[Operation::AddLayer, Operation::TranslateLayer { layer: 1, dx: 1, dy: 0, clip: false }], 1).is_err());
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
