use std::io::Read;
use std::path::Path;
use std::time::Duration;

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

use crate::app_paths;
use crate::db;
use crate::types::{
    GenerateImageRequest, Generation, GenerationDetail, GenerationInputImage, GenerationOutput,
    InputImageRequest, StoredProviderProfile,
};

const MAX_EDIT_IMAGES: usize = 16;
const MAX_EDIT_IMAGE_BYTES: usize = 50 * 1024 * 1024;

pub struct OpenAiJob {
    pub generation: Generation,
    pub base_url: String,
    pub api_key: String,
    pub debug_mode: bool,
    pub timeout_seconds: u64,
    pub endpoint: OpenAiEndpoint,
    pub params: Value,
    pub input_images: Vec<PreparedInputImage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiEndpoint {
    Generation,
    Edit,
}

pub struct PreparedInputImage {
    name: String,
    mime_type: String,
    bytes: Vec<u8>,
}

pub fn create_job(
    request: GenerateImageRequest,
    profile: StoredProviderProfile,
    api_key: String,
) -> Result<OpenAiJob, String> {
    let timeout_minutes = validate_timeout_minutes(
        request
            .network_timeout_minutes
            .unwrap_or(profile.profile.network_timeout_minutes),
    )?;
    let normalized = NormalizedRequest::from_request(request)?;
    let now = crate::commands::now_millis();
    let request_json = request_record_json(
        &profile.profile.base_url,
        normalized.endpoint,
        &normalized.history_params,
        &normalized.input_images,
        timeout_minutes,
    )?;
    let generation = Generation {
        id: crate::commands::make_id("generation"),
        prompt: normalized.prompt.clone(),
        provider_id: profile.profile.id.clone(),
        provider_type: profile.profile.provider_type.clone(),
        provider_name: profile.profile.name.clone(),
        model: normalized.model.clone(),
        status: "running".to_string(),
        size: normalized.size.clone(),
        quality: normalized.quality.clone(),
        output_format: normalized.output_format.clone(),
        params_json: request_json,
        response_json: None,
        error_message: None,
        revised_prompt: None,
        created_at: now,
        completed_at: None,
    };

    Ok(OpenAiJob {
        generation,
        base_url: profile.profile.base_url,
        api_key,
        debug_mode: normalized.debug_mode,
        timeout_seconds: timeout_minutes as u64 * 60,
        endpoint: normalized.endpoint,
        params: normalized.api_params,
        input_images: normalized.input_images,
    })
}

pub fn run_job(job: OpenAiJob) -> Result<GenerationDetail, String> {
    run_job_inner(job, true)
}

pub fn run_existing_job(job: OpenAiJob) -> Result<GenerationDetail, String> {
    run_job_inner(job, false)
}

fn run_job_inner(job: OpenAiJob, insert_generation: bool) -> Result<GenerationDetail, String> {
    let normalized = NormalizedJob {
        output_format: job.generation.output_format.clone(),
        debug_mode: job.debug_mode,
        timeout_seconds: job.timeout_seconds,
        endpoint: job.endpoint,
        params: job.params,
        input_images: job.input_images,
    };
    let generation = job.generation;
    if insert_generation {
        db::insert_generation(&generation)?;
    }
    let persisted_inputs = match persist_input_images(&generation.id, &normalized.input_images) {
        Ok(inputs) => inputs,
        Err(message) => {
            let _ = db::update_generation_failed(
                &generation.id,
                &message,
                None,
                crate::commands::now_millis(),
            );
            return Err(message);
        }
    };
    let debug_dir = if normalized.debug_mode {
        Some(app_paths::generation_debug_dir(&generation.id)?)
    } else {
        None
    };

    let result = call_openai(
        &job.base_url,
        &job.api_key,
        &normalized,
        debug_dir.as_deref(),
    )
    .and_then(|response| {
        persist_outputs(
            &generation.id,
            &normalized.output_format,
            response.parsed,
            normalized.timeout_seconds,
        )
        .map(|(outputs, revised_prompt)| (outputs, revised_prompt, response.body))
        .map_err(OpenAiCallError::new)
    });

    match result {
        Ok((outputs, revised_prompt, response_json)) => {
            let completed_at = crate::commands::now_millis();
            db::update_generation_success(
                &generation.id,
                revised_prompt.as_deref(),
                Some(&response_json),
                completed_at,
            )?;
            db::get_generation_detail(&generation.id)?
                .ok_or_else(|| "Generation was saved but could not be reloaded".to_string())
                .map(|mut detail| {
                    if detail.outputs.is_empty() {
                        detail.outputs = outputs;
                    }
                    if detail.input_images.is_empty() {
                        detail.input_images = persisted_inputs;
                    }
                    detail
                })
        }
        Err(err) => {
            let err = err.with_debug_path(debug_dir.as_deref());
            let _ = db::update_generation_failed(
                &generation.id,
                &err.message,
                err.response_json.as_deref(),
                crate::commands::now_millis(),
            );
            Err(err.message)
        }
    }
}

fn call_openai(
    base_url: &str,
    api_key: &str,
    normalized: &NormalizedJob,
    debug_dir: Option<&Path>,
) -> Result<OpenAiCallResponse, OpenAiCallError> {
    match normalized.endpoint {
        OpenAiEndpoint::Generation => {
            call_openai_generation(base_url, api_key, normalized, debug_dir)
        }
        OpenAiEndpoint::Edit => call_openai_edit(base_url, api_key, normalized, debug_dir),
    }
}

fn call_openai_generation(
    base_url: &str,
    api_key: &str,
    normalized: &NormalizedJob,
    debug_dir: Option<&Path>,
) -> Result<OpenAiCallResponse, OpenAiCallError> {
    let url = image_generation_url(base_url)?;
    let payload = serde_json::to_string(&normalized.params).map_err(|err| err.to_string())?;
    write_debug_json(
        debug_dir,
        "request.json",
        &json!({
            "method": "POST",
            "url": url,
            "headers": {
                "authorization": "Bearer <redacted>",
                "content-type": "application/json"
            },
            "body": normalized.params
        }),
    );
    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(normalized.timeout_seconds))
        .send_string(&payload);

    let body = match response {
        Ok(response) => {
            let status = response.status();
            let body = response.into_string().map_err(|err| err.to_string())?;
            write_debug_response(debug_dir, "response", status, &body);
            body
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            write_debug_response(debug_dir, "http-error-response", code, &body);
            return Err(OpenAiCallError::with_response(
                format!("Image API returned {code}: {}", api_error_message(&body)),
                response_body_json(&body),
            ));
        }
        Err(ureq::Error::Transport(err)) => {
            write_debug_text(debug_dir, "transport-error.txt", &err.to_string());
            return Err(OpenAiCallError::new(format!(
                "Image API request failed: {err}"
            )));
        }
    };

    let parsed = parse_image_response(&body).map_err(OpenAiCallError::new)?;
    Ok(OpenAiCallResponse {
        parsed,
        body: response_body_json(&body),
    })
}

fn call_openai_edit(
    base_url: &str,
    api_key: &str,
    normalized: &NormalizedJob,
    debug_dir: Option<&Path>,
) -> Result<OpenAiCallResponse, OpenAiCallError> {
    let url = image_edit_url(base_url)?;
    let (body, boundary) = build_edit_multipart(&normalized.params, &normalized.input_images)?;
    write_debug_json(
        debug_dir,
        "request.json",
        &json!({
            "method": "POST",
            "url": url,
            "headers": {
                "authorization": "Bearer <redacted>",
                "content-type": format!("multipart/form-data; boundary={boundary}")
            },
            "body": normalized.params,
            "input_images": normalized
                .input_images
                .iter()
                .map(|image| json!({
                    "name": image.name,
                    "mime_type": image.mime_type,
                    "bytes": image.bytes.len()
                }))
                .collect::<Vec<_>>()
        }),
    );
    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .timeout(Duration::from_secs(normalized.timeout_seconds))
        .send_bytes(&body);

    let body = match response {
        Ok(response) => {
            let status = response.status();
            let body = response.into_string().map_err(|err| err.to_string())?;
            write_debug_response(debug_dir, "response", status, &body);
            body
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            write_debug_response(debug_dir, "http-error-response", code, &body);
            return Err(OpenAiCallError::with_response(
                format!(
                    "Image edit API returned {code}: {}",
                    api_error_message(&body)
                ),
                response_body_json(&body),
            ));
        }
        Err(ureq::Error::Transport(err)) => {
            write_debug_text(debug_dir, "transport-error.txt", &err.to_string());
            return Err(OpenAiCallError::new(format!(
                "Image edit API request failed: {err}"
            )));
        }
    };

    let parsed = parse_image_response(&body).map_err(OpenAiCallError::new)?;
    Ok(OpenAiCallResponse {
        parsed,
        body: response_body_json(&body),
    })
}

fn persist_outputs(
    generation_id: &str,
    output_format: &str,
    response: OpenAiImageResponse,
    timeout_seconds: u64,
) -> Result<(Vec<GenerationOutput>, Option<String>), String> {
    if response.data.is_empty() {
        return Err("Image API response did not include any image data".to_string());
    }

    let mut outputs = Vec::new();
    let now = crate::commands::now_millis();
    let dir = app_paths::generation_image_dir(now)?;
    let revised_prompt = response
        .data
        .iter()
        .find_map(|item| item.revised_prompt.clone());

    for (index, item) in response.data.into_iter().enumerate() {
        let payload = item.payload.ok_or_else(|| {
            "Image API response item did not include b64_json, url, base64, image, or another supported image field".to_string()
        })?;
        let (bytes, detected_format) = image_bytes_from_payload(payload, timeout_seconds)?;
        let item_format = detected_format.unwrap_or_else(|| output_format.to_string());
        let item_extension = extension_for_format(&item_format);
        let path = dir.join(format!("{generation_id}-{index}.{item_extension}"));
        std::fs::write(&path, &bytes).map_err(|err| err.to_string())?;
        let (width, height) = read_dimensions(&bytes, &item_format);
        let output = GenerationOutput {
            id: 0,
            generation_id: generation_id.to_string(),
            path: path.to_string_lossy().to_string(),
            format: item_format,
            width,
            height,
            file_size: bytes.len() as i64,
            output_index: index as i64,
            created_at: now,
        };
        db::insert_output(&output)?;
        outputs.push(output);
    }

    Ok((outputs, revised_prompt))
}

fn persist_input_images(
    generation_id: &str,
    input_images: &[PreparedInputImage],
) -> Result<Vec<GenerationInputImage>, String> {
    if input_images.is_empty() {
        return Ok(Vec::new());
    }

    let now = crate::commands::now_millis();
    let dir = app_paths::generation_image_dir(now)?;
    let mut persisted = Vec::new();

    for (index, image) in input_images.iter().enumerate() {
        let format = image
            .mime_type
            .strip_prefix("image/")
            .map(normalize_format)
            .unwrap_or_else(|| "png".to_string());
        let extension = extension_for_format(&format);
        let path = dir.join(format!("{generation_id}-input-{index}.{extension}"));
        std::fs::write(&path, &image.bytes).map_err(|err| err.to_string())?;

        let input = GenerationInputImage {
            id: 0,
            generation_id: generation_id.to_string(),
            path: path.to_string_lossy().to_string(),
            name: image.name.clone(),
            mime_type: image.mime_type.clone(),
            file_size: image.bytes.len() as i64,
            input_index: index as i64,
            created_at: now,
        };
        db::insert_input_image(&input)?;
        persisted.push(input);
    }

    Ok(persisted)
}

pub fn image_generation_url(base_url: &str) -> Result<String, String> {
    image_endpoint_url(base_url, "generations")
}

pub fn image_edit_url(base_url: &str) -> Result<String, String> {
    image_endpoint_url(base_url, "edits")
}

fn request_record_json(
    base_url: &str,
    endpoint: OpenAiEndpoint,
    params: &Value,
    input_images: &[PreparedInputImage],
    timeout_minutes: i64,
) -> Result<String, String> {
    let url = match endpoint {
        OpenAiEndpoint::Generation => image_generation_url(base_url)?,
        OpenAiEndpoint::Edit => image_edit_url(base_url)?,
    };
    let content_type = match endpoint {
        OpenAiEndpoint::Generation => "application/json".to_string(),
        OpenAiEndpoint::Edit => "multipart/form-data".to_string(),
    };
    let value = json!({
        "method": "POST",
        "url": url,
        "headers": {
            "authorization": "Bearer <redacted>",
            "content-type": content_type
        },
        "timeout_minutes": timeout_minutes,
        "body": params,
        "input_images": input_images
            .iter()
            .map(|image| json!({
                "name": image.name,
                "mime_type": image.mime_type,
                "bytes": image.bytes.len()
            }))
            .collect::<Vec<_>>()
    });
    serde_json::to_string_pretty(&value).map_err(|err| err.to_string())
}

fn image_endpoint_url(base_url: &str, endpoint: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Base URL is required".to_string());
    }
    if trimmed.ends_with("/images/generations") || trimmed.ends_with("/images/edits") {
        let base = trimmed
            .strip_suffix("/images/generations")
            .or_else(|| trimmed.strip_suffix("/images/edits"))
            .unwrap_or(trimmed);
        return Ok(format!("{base}/images/{endpoint}"));
    }
    Ok(format!("{trimmed}/images/{endpoint}"))
}

fn response_body_json(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| body.to_string())
}

fn validate_timeout_minutes(value: i64) -> Result<i64, String> {
    if !(1..=120).contains(&value) {
        return Err("Network timeout must be between 1 and 120 minutes".to_string());
    }
    Ok(value)
}

fn api_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|message| message.as_str())
                .map(ToString::to_string)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| body.trim().to_string())
}

fn parse_image_response(body: &str) -> Result<OpenAiImageResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|err| format!("Image API returned a response that was not valid JSON: {err}"))?;
    let items = response_items(&value);
    if items.is_empty() {
        return Err(
            "Image API response did not include data, images, output, or a top-level image field"
                .to_string(),
        );
    }
    let data = items
        .into_iter()
        .map(|item| OpenAiImageItem {
            payload: image_payload_from_value(item),
            revised_prompt: item
                .get("revised_prompt")
                .or_else(|| item.get("revisedPrompt"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
        })
        .collect();
    Ok(OpenAiImageResponse { data })
}

struct OpenAiImageResponse {
    data: Vec<OpenAiImageItem>,
}

struct OpenAiImageItem {
    payload: Option<ImagePayload>,
    revised_prompt: Option<String>,
}

struct OpenAiCallResponse {
    parsed: OpenAiImageResponse,
    body: String,
}

struct OpenAiCallError {
    message: String,
    response_json: Option<String>,
}

impl OpenAiCallError {
    fn new(message: String) -> Self {
        Self {
            message,
            response_json: None,
        }
    }

    fn with_response(message: String, response_json: String) -> Self {
        Self {
            message,
            response_json: Some(response_json),
        }
    }

    fn with_debug_path(mut self, debug_dir: Option<&Path>) -> Self {
        if let Some(debug_dir) = debug_dir {
            self.message = format!("{}. Debug files: {}", self.message, debug_dir.display());
        }
        self
    }
}

impl From<String> for OpenAiCallError {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

enum ImagePayload {
    Base64(String),
    Url(String),
}

struct NormalizedRequest {
    model: String,
    prompt: String,
    size: String,
    quality: String,
    output_format: String,
    debug_mode: bool,
    endpoint: OpenAiEndpoint,
    api_params: Value,
    history_params: Value,
    input_images: Vec<PreparedInputImage>,
}

struct NormalizedJob {
    output_format: String,
    debug_mode: bool,
    timeout_seconds: u64,
    endpoint: OpenAiEndpoint,
    params: Value,
    input_images: Vec<PreparedInputImage>,
}

impl NormalizedRequest {
    fn from_request(request: GenerateImageRequest) -> Result<Self, String> {
        let model = required_trimmed(&request.model, "Model")?;
        let prompt = required_trimmed(&request.prompt, "Prompt")?;
        let size = validate_size(&request.size)?;
        let quality = validate_quality(&request.quality)?;
        let output_format = validate_output_format(&request.output_format)?;
        let moderation = validate_moderation(request.moderation.as_deref())?;
        let compression = validate_compression(request.output_compression, &output_format)?;
        let input_images = prepare_input_images(request.input_images.unwrap_or_default())?;
        let endpoint = if input_images.is_empty() {
            OpenAiEndpoint::Generation
        } else {
            OpenAiEndpoint::Edit
        };

        let mut api_params = json!({
            "model": model,
            "prompt": prompt,
            "size": size,
            "quality": quality,
            "output_format": output_format,
            "moderation": moderation
        });

        if let Some(compression) = compression {
            api_params["output_compression"] = json!(compression);
        }

        let mut history_params = api_params.clone();
        history_params["mode"] = json!(match endpoint {
            OpenAiEndpoint::Generation => "generate",
            OpenAiEndpoint::Edit => "edit",
        });
        if endpoint == OpenAiEndpoint::Edit {
            history_params["input_images"] = json!(input_images
                .iter()
                .map(|image| json!({
                    "name": image.name,
                    "mime_type": image.mime_type,
                    "bytes": image.bytes.len()
                }))
                .collect::<Vec<_>>());
        }

        Ok(Self {
            model,
            prompt,
            size,
            quality,
            output_format,
            debug_mode: request.debug_mode.unwrap_or(false),
            endpoint,
            api_params,
            history_params,
            input_images,
        })
    }
}

fn prepare_input_images(images: Vec<InputImageRequest>) -> Result<Vec<PreparedInputImage>, String> {
    if images.len() > MAX_EDIT_IMAGES {
        return Err(format!(
            "Image edit supports up to {MAX_EDIT_IMAGES} input images"
        ));
    }

    images
        .into_iter()
        .enumerate()
        .map(|(index, image)| prepare_input_image(index, image))
        .collect()
}

fn prepare_input_image(
    index: usize,
    image: InputImageRequest,
) -> Result<PreparedInputImage, String> {
    let (bytes, data_url_mime) = decode_input_image_data(&image.data_url)?;
    if bytes.is_empty() {
        return Err("Input image is empty".to_string());
    }
    if bytes.len() > MAX_EDIT_IMAGE_BYTES {
        return Err("Each input image must be 50MB or smaller".to_string());
    }

    let mime_type = normalize_input_mime(
        image
            .mime_type
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or(data_url_mime.as_deref()),
        &bytes,
    )?;
    let format = mime_type
        .strip_prefix("image/")
        .map(normalize_format)
        .unwrap_or_else(|| "png".to_string());
    let extension = extension_for_format(&format);
    let name = sanitize_file_name(
        image
            .name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("input"),
    );
    let name = if Path::new(&name).extension().is_some() {
        name
    } else {
        format!("{name}-{index}.{extension}")
    };

    Ok(PreparedInputImage {
        name,
        mime_type,
        bytes,
    })
}

fn decode_input_image_data(value: &str) -> Result<(Vec<u8>, Option<String>), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Input image data is required".to_string());
    }

    let mime = mime_from_data_url(value);
    let bytes = decode_image_base64(value)?;
    Ok((bytes, mime))
}

fn mime_from_data_url(value: &str) -> Option<String> {
    if !value.starts_with("data:") {
        return None;
    }
    let header = value.split_once(',')?.0;
    header
        .strip_prefix("data:")
        .and_then(|value| value.split(';').next())
        .filter(|value| value.starts_with("image/"))
        .map(normalize_mime_type)
}

fn normalize_input_mime(value: Option<&str>, bytes: &[u8]) -> Result<String, String> {
    if let Some(value) = value {
        let normalized = normalize_mime_type(value);
        if is_supported_input_mime(&normalized) {
            return Ok(normalized);
        }
    }

    let inferred = infer_format_from_bytes(bytes)
        .map(mime_type_for_format)
        .filter(|value| is_supported_input_mime(value))
        .map(ToString::to_string);
    inferred.ok_or_else(|| "Input images must be PNG, JPEG, or WebP".to_string())
}

fn normalize_mime_type(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "image/jpg" | "image/pjpeg" => "image/jpeg".to_string(),
        "image/png" => "image/png".to_string(),
        "image/jpeg" => "image/jpeg".to_string(),
        "image/webp" => "image/webp".to_string(),
        other => other.to_string(),
    }
}

fn is_supported_input_mime(value: &str) -> bool {
    matches!(value, "image/png" | "image/jpeg" | "image/webp")
}

fn mime_type_for_format(format: &str) -> &'static str {
    match normalize_format(format).as_str() {
        "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

fn sanitize_file_name(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches(['.', '-']);
    if sanitized.is_empty() {
        "input".to_string()
    } else {
        sanitized.to_string()
    }
}

fn build_edit_multipart(
    params: &Value,
    input_images: &[PreparedInputImage],
) -> Result<(Vec<u8>, String), String> {
    if input_images.is_empty() {
        return Err("At least one input image is required for image edits".to_string());
    }

    let boundary = format!(
        "image-gen-kit-{}-{}",
        crate::commands::now_millis(),
        input_images.len()
    );
    let mut body = Vec::new();

    let fields = params
        .as_object()
        .ok_or_else(|| "Image edit parameters must be an object".to_string())?;
    for (name, value) in fields {
        if let Some(value) = multipart_field_value(value) {
            push_multipart_field(&mut body, &boundary, name, &value);
        }
    }

    for image in input_images {
        push_multipart_file(&mut body, &boundary, "image[]", image);
    }

    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((body, boundary))
}

fn multipart_field_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn push_multipart_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
            escape_multipart_header(name)
        )
        .as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

fn push_multipart_file(body: &mut Vec<u8>, boundary: &str, name: &str, image: &PreparedInputImage) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
            escape_multipart_header(name),
            escape_multipart_header(&image.name)
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", image.mime_type).as_bytes());
    body.extend_from_slice(&image.bytes);
    body.extend_from_slice(b"\r\n");
}

fn escape_multipart_header(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn response_items(value: &Value) -> Vec<&Value> {
    for key in ["data", "images", "output", "outputs", "result", "results"] {
        if let Some(found) = value.get(key) {
            if let Some(items) = values_as_items(found) {
                return items;
            }
        }
    }

    if image_payload_from_value(value).is_some() {
        vec![value]
    } else {
        Vec::new()
    }
}

fn values_as_items(value: &Value) -> Option<Vec<&Value>> {
    if let Some(items) = value.as_array() {
        return Some(items.iter().collect());
    }
    if value.is_object() {
        return Some(vec![value]);
    }
    None
}

fn image_payload_from_value(value: &Value) -> Option<ImagePayload> {
    if let Some(text) = value.as_str() {
        return image_payload_from_string(text, false);
    }

    let object = value.as_object()?;
    for key in [
        "b64_json",
        "base64",
        "b64",
        "image_base64",
        "imageBase64",
        "image",
        "data",
        "result",
    ] {
        if let Some(text) = object.get(key).and_then(|value| value.as_str()) {
            if let Some(payload) = image_payload_from_string(text, true) {
                return Some(payload);
            }
        }
    }

    for key in ["url", "image_url", "imageUrl", "output_url", "outputUrl"] {
        if let Some(value) = object.get(key) {
            if let Some(text) = value.as_str() {
                if let Some(payload) = image_payload_from_string(text, false) {
                    return Some(payload);
                }
            }
            if let Some(text) = value.get("url").and_then(|value| value.as_str()) {
                if let Some(payload) = image_payload_from_string(text, false) {
                    return Some(payload);
                }
            }
        }
    }

    for key in ["image", "content", "message"] {
        if let Some(value) = object.get(key) {
            if let Some(payload) = image_payload_from_value(value) {
                return Some(payload);
            }
            if let Some(items) = value.as_array() {
                for item in items {
                    if let Some(payload) = image_payload_from_value(item) {
                        return Some(payload);
                    }
                }
            }
        }
    }

    None
}

fn image_payload_from_string(value: &str, trust_base64: bool) -> Option<ImagePayload> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Some(ImagePayload::Url(value.to_string()));
    }
    if value.starts_with("data:image/") {
        return Some(ImagePayload::Base64(value.to_string()));
    }
    if trust_base64 || decodes_to_known_image(value) {
        return Some(ImagePayload::Base64(value.to_string()));
    }
    None
}

fn image_bytes_from_payload(
    payload: ImagePayload,
    timeout_seconds: u64,
) -> Result<(Vec<u8>, Option<String>), String> {
    match payload {
        ImagePayload::Base64(value) => {
            let format = format_from_data_url(&value);
            let bytes = decode_image_base64(&value)?;
            let detected = infer_format_from_bytes(&bytes)
                .map(ToString::to_string)
                .or(format);
            Ok((bytes, detected))
        }
        ImagePayload::Url(url) => download_image(&url, timeout_seconds),
    }
}

fn decode_image_base64(value: &str) -> Result<Vec<u8>, String> {
    let base64_value = strip_data_url_prefix(value).replace(['\n', '\r', ' ', '\t'], "");
    general_purpose::STANDARD
        .decode(&base64_value)
        .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(&base64_value))
        .map_err(|err| format!("Image API returned invalid base64 image data: {err}"))
}

fn strip_data_url_prefix(value: &str) -> &str {
    if value.starts_with("data:image/") {
        value.split_once(',').map(|(_, data)| data).unwrap_or(value)
    } else if value.starts_with("data:") {
        value.split_once(',').map(|(_, data)| data).unwrap_or(value)
    } else {
        value
    }
}

fn format_from_data_url(value: &str) -> Option<String> {
    if !value.starts_with("data:image/") {
        return None;
    }
    let mime = value.split_once(';')?.0;
    Some(normalize_format(mime.strip_prefix("data:image/")?))
}

fn decodes_to_known_image(value: &str) -> bool {
    decode_image_base64(value)
        .ok()
        .and_then(|bytes| infer_format_from_bytes(&bytes).map(ToString::to_string))
        .is_some()
}

fn download_image(url: &str, timeout_seconds: u64) -> Result<(Vec<u8>, Option<String>), String> {
    let response = ureq::get(url)
        .timeout(Duration::from_secs(timeout_seconds))
        .call()
        .map_err(|err| format!("Unable to download image URL returned by API: {err}"))?;
    let content_type = response.header("content-type").map(ToString::to_string);
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| format!("Unable to read image URL returned by API: {err}"))?;
    if bytes.is_empty() {
        return Err("Image URL returned by API downloaded an empty body".to_string());
    }
    let detected = infer_format_from_bytes(&bytes)
        .map(ToString::to_string)
        .or_else(|| {
            content_type
                .as_deref()
                .and_then(|value| value.strip_prefix("image/"))
                .map(normalize_format)
        });
    Ok((bytes, detected))
}

fn required_trimmed(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value.to_string())
    }
}

pub fn validate_size(value: &str) -> Result<String, String> {
    let value = value.trim().to_lowercase();
    if value == "auto" {
        return Ok(value);
    }
    let parts: Vec<&str> = value.split('x').collect();
    if parts.len() != 2 {
        return Err("Size must be auto or WIDTHxHEIGHT".to_string());
    }
    let width: i64 = parts[0]
        .parse()
        .map_err(|_| "Size width must be a number".to_string())?;
    let height: i64 = parts[1]
        .parse()
        .map_err(|_| "Size height must be a number".to_string())?;
    if width <= 0 || height <= 0 {
        return Err("Size dimensions must be positive".to_string());
    }
    if width > 3840 || height > 3840 {
        return Err("Maximum image edge is 3840px".to_string());
    }
    if width % 16 != 0 || height % 16 != 0 {
        return Err("Both image edges must be multiples of 16px".to_string());
    }
    let long = width.max(height);
    let short = width.min(height);
    if long > short * 3 {
        return Err("Long edge to short edge ratio must not exceed 3:1".to_string());
    }
    let pixels = width * height;
    if !(655_360..=8_294_400).contains(&pixels) {
        return Err("Total pixels must be between 655,360 and 8,294,400".to_string());
    }
    Ok(format!("{width}x{height}"))
}

fn validate_quality(value: &str) -> Result<String, String> {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "auto" | "low" | "medium" | "high" => Ok(value),
        _ => Err("Quality must be auto, low, medium, or high".to_string()),
    }
}

fn validate_output_format(value: &str) -> Result<String, String> {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "png" | "jpeg" | "webp" => Ok(value),
        _ => Err("Output format must be png, jpeg, or webp".to_string()),
    }
}

fn validate_moderation(value: Option<&str>) -> Result<String, String> {
    let value = value.unwrap_or("auto").trim().to_lowercase();
    match value.as_str() {
        "auto" | "low" => Ok(value),
        _ => Err("Moderation must be auto or low".to_string()),
    }
}

fn validate_compression(value: Option<i64>, output_format: &str) -> Result<Option<i64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if output_format == "png" {
        return Ok(None);
    }
    if !(0..=100).contains(&value) {
        return Err("Output compression must be between 0 and 100".to_string());
    }
    Ok(Some(value))
}

fn write_debug_json(debug_dir: Option<&Path>, file_name: &str, value: &Value) {
    let Some(debug_dir) = debug_dir else {
        return;
    };
    let Ok(content) = serde_json::to_string_pretty(value) else {
        return;
    };
    let _ = std::fs::write(debug_dir.join(file_name), content);
}

fn write_debug_response(debug_dir: Option<&Path>, prefix: &str, status: u16, body: &str) {
    let Some(debug_dir) = debug_dir else {
        return;
    };
    let _ = std::fs::write(debug_dir.join(format!("{prefix}.txt")), body);
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        write_debug_json(debug_dir.into(), &format!("{prefix}.json"), &value);
    }
    let _ = std::fs::write(
        debug_dir.join(format!("{prefix}-status.txt")),
        status.to_string(),
    );
}

fn write_debug_text(debug_dir: Option<&Path>, file_name: &str, value: &str) {
    let Some(debug_dir) = debug_dir else {
        return;
    };
    let _ = std::fs::write(debug_dir.join(file_name), value);
}

fn extension_for_format(output_format: &str) -> &'static str {
    match normalize_format(output_format).as_str() {
        "jpeg" => "jpg",
        "webp" => "webp",
        _ => "png",
    }
}

fn normalize_format(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "jpg" | "jpeg" | "pjpeg" => "jpeg".to_string(),
        "webp" => "webp".to_string(),
        "png" | "x-png" => "png".to_string(),
        "gif" => "gif".to_string(),
        other => other.to_string(),
    }
}

fn infer_format_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Some("png");
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return Some("jpeg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        return Some("gif");
    }
    None
}

fn read_dimensions(bytes: &[u8], output_format: &str) -> (Option<i64>, Option<i64>) {
    match normalize_format(output_format).as_str() {
        "png" => read_png_dimensions(bytes),
        "jpeg" => read_jpeg_dimensions(bytes),
        _ => (None, None),
    }
}

fn read_png_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return (None, None);
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as i64;
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as i64;
    (Some(width), Some(height))
}

fn read_jpeg_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return (None, None);
    }
    let mut index = 2;
    while index + 9 < bytes.len() {
        if bytes[index] != 0xff {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        index += 2;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if index + 2 > bytes.len() {
            break;
        }
        let length = u16::from_be_bytes([bytes[index], bytes[index + 1]]) as usize;
        if length < 2 || index + length > bytes.len() {
            break;
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            let height = u16::from_be_bytes([bytes[index + 3], bytes[index + 4]]) as i64;
            let width = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]) as i64;
            return (Some(width), Some(height));
        }
        index += length;
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::{
        build_edit_multipart, image_edit_url, image_generation_url, parse_image_response,
        validate_size, ImagePayload, PreparedInputImage,
    };

    #[test]
    fn accepts_openai_size_constraints() {
        assert_eq!(validate_size("1024x1024").unwrap(), "1024x1024");
        assert_eq!(validate_size("1536x1024").unwrap(), "1536x1024");
        assert_eq!(validate_size("1024x1536").unwrap(), "1024x1536");
        assert_eq!(validate_size("2048x2048").unwrap(), "2048x2048");
        assert_eq!(validate_size("2048x1152").unwrap(), "2048x1152");
        assert_eq!(validate_size("3840x2160").unwrap(), "3840x2160");
        assert_eq!(validate_size("2160x3840").unwrap(), "2160x3840");
        assert_eq!(validate_size("AUTO").unwrap(), "auto");
    }

    #[test]
    fn rejects_invalid_size_constraints() {
        assert!(validate_size("1000x1000").is_err());
        assert!(validate_size("4096x1024").is_err());
        assert!(validate_size("3840x1008").is_err());
    }

    #[test]
    fn builds_generation_endpoint_from_base_url() {
        assert_eq!(
            image_generation_url("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1/images/generations"
        );
        assert_eq!(
            image_generation_url("https://example.test/v1/images/generations").unwrap(),
            "https://example.test/v1/images/generations"
        );
    }

    #[test]
    fn builds_edit_endpoint_from_base_url() {
        assert_eq!(
            image_edit_url("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1/images/edits"
        );
        assert_eq!(
            image_edit_url("https://example.test/v1/images/generations").unwrap(),
            "https://example.test/v1/images/edits"
        );
    }

    #[test]
    fn builds_edit_multipart_with_image_array_field() {
        let image = PreparedInputImage {
            name: "input.png".to_string(),
            mime_type: "image/png".to_string(),
            bytes: b"png-bytes".to_vec(),
        };
        let (body, boundary) = build_edit_multipart(
            &serde_json::json!({
                "model": "gpt-image-2",
                "prompt": "make it cinematic",
                "size": "auto"
            }),
            &[image],
        )
        .unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains(&format!("--{boundary}")));
        assert!(text.contains("name=\"prompt\""));
        assert!(text.contains("name=\"image[]\"; filename=\"input.png\""));
        assert!(text.contains("Content-Type: image/png"));
    }

    #[test]
    fn parses_url_image_response_from_compatible_provider() {
        let parsed = parse_image_response(
            r#"{"data":[{"url":"https://cdn.example.test/image.png","revised_prompt":"ok"}]}"#,
        )
        .unwrap();
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(parsed.data[0].revised_prompt.as_deref(), Some("ok"));
        assert!(matches!(
            parsed.data[0].payload,
            Some(ImagePayload::Url(ref url)) if url == "https://cdn.example.test/image.png"
        ));
    }

    #[test]
    fn parses_nonstandard_base64_image_field() {
        let parsed =
            parse_image_response(r#"{"images":[{"image":"data:image/png;base64,iVBORw0KGgo="}]}"#)
                .unwrap();
        assert_eq!(parsed.data.len(), 1);
        assert!(matches!(
            parsed.data[0].payload,
            Some(ImagePayload::Base64(_))
        ));
    }
}
