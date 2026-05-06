use std::time::Duration;

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

use crate::app_paths;
use crate::db;
use crate::types::{
    GenerateImageRequest, Generation, GenerationDetail, GenerationOutput, StoredProviderProfile,
};

pub fn generate(
    request: GenerateImageRequest,
    profile: StoredProviderProfile,
    api_key: String,
) -> Result<GenerationDetail, String> {
    let normalized = NormalizedRequest::from_request(request)?;
    let now = crate::commands::now_millis();
    let params_json = serde_json::to_string(&normalized.params).map_err(|err| err.to_string())?;
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
        params_json,
        error_message: None,
        revised_prompt: None,
        created_at: now,
        completed_at: None,
    };

    db::insert_generation(&generation)?;
    match call_openai(&profile.profile.base_url, &api_key, &normalized)
        .and_then(|response| persist_outputs(&generation.id, &normalized.output_format, response))
    {
        Ok((outputs, revised_prompt)) => {
            let completed_at = crate::commands::now_millis();
            db::update_generation_success(&generation.id, revised_prompt.as_deref(), completed_at)?;
            db::get_generation_detail(&generation.id)?
                .ok_or_else(|| "Generation was saved but could not be reloaded".to_string())
                .map(|mut detail| {
                    if detail.outputs.is_empty() {
                        detail.outputs = outputs;
                    }
                    detail
                })
        }
        Err(err) => {
            let _ =
                db::update_generation_failed(&generation.id, &err, crate::commands::now_millis());
            Err(err)
        }
    }
}

fn call_openai(
    base_url: &str,
    api_key: &str,
    normalized: &NormalizedRequest,
) -> Result<OpenAiImageResponse, String> {
    let url = image_generation_url(base_url)?;
    let payload = serde_json::to_string(&normalized.params).map_err(|err| err.to_string())?;
    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(180))
        .send_string(&payload);

    let body = match response {
        Ok(response) => response.into_string().map_err(|err| err.to_string())?,
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            return Err(format!(
                "Image API returned {code}: {}",
                api_error_message(&body)
            ));
        }
        Err(ureq::Error::Transport(err)) => return Err(format!("Image API request failed: {err}")),
    };

    parse_image_response(&body)
}

fn persist_outputs(
    generation_id: &str,
    output_format: &str,
    response: OpenAiImageResponse,
) -> Result<(Vec<GenerationOutput>, Option<String>), String> {
    if response.data.is_empty() {
        return Err("Image API response did not include any image data".to_string());
    }

    let mut outputs = Vec::new();
    let now = crate::commands::now_millis();
    let dir = app_paths::generation_image_dir(now)?;
    let extension = extension_for_format(output_format);
    let revised_prompt = response
        .data
        .iter()
        .find_map(|item| item.revised_prompt.clone());

    for (index, item) in response.data.into_iter().enumerate() {
        let image_base64 = item
            .b64_json
            .ok_or_else(|| "Image API response item did not include b64_json".to_string())?;
        let bytes = general_purpose::STANDARD
            .decode(image_base64)
            .map_err(|err| format!("Image API returned invalid base64: {err}"))?;
        let path = dir.join(format!("{generation_id}-{index}.{extension}"));
        std::fs::write(&path, &bytes).map_err(|err| err.to_string())?;
        let (width, height) = read_dimensions(&bytes, output_format);
        let output = GenerationOutput {
            id: 0,
            generation_id: generation_id.to_string(),
            path: path.to_string_lossy().to_string(),
            format: output_format.to_string(),
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

pub fn image_generation_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Base URL is required".to_string());
    }
    if trimmed.ends_with("/images/generations") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("{trimmed}/images/generations"))
    }
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
    let data = value
        .get("data")
        .and_then(|data| data.as_array())
        .ok_or_else(|| "Image API response did not include a data array".to_string())?
        .iter()
        .map(|item| OpenAiImageItem {
            b64_json: item
                .get("b64_json")
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            revised_prompt: item
                .get("revised_prompt")
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
    b64_json: Option<String>,
    revised_prompt: Option<String>,
}

struct NormalizedRequest {
    model: String,
    prompt: String,
    size: String,
    quality: String,
    output_format: String,
    params: Value,
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

        let mut params = json!({
            "model": model,
            "prompt": prompt,
            "size": size,
            "quality": quality,
            "output_format": output_format,
            "moderation": moderation
        });

        if let Some(compression) = compression {
            params["output_compression"] = json!(compression);
        }

        Ok(Self {
            model,
            prompt,
            size,
            quality,
            output_format,
            params,
        })
    }
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

fn extension_for_format(output_format: &str) -> &'static str {
    match output_format {
        "jpeg" => "jpg",
        "webp" => "webp",
        _ => "png",
    }
}

fn read_dimensions(bytes: &[u8], output_format: &str) -> (Option<i64>, Option<i64>) {
    match output_format {
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
    use super::{image_generation_url, validate_size};

    #[test]
    fn accepts_openai_size_constraints() {
        assert_eq!(validate_size("1024x1024").unwrap(), "1024x1024");
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
}
