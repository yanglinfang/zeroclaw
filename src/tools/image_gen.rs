use super::traits::{Tool, ToolResult};
use crate::security::policy::ToolOperation;
use crate::security::SecurityPolicy;
use anyhow::Context;
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

/// Standalone image generation tool using fal.ai (Flux / Nano Banana models).
///
/// Reads the API key from an environment variable (default: `FAL_API_KEY`),
/// calls the fal.ai synchronous endpoint, downloads the resulting image,
/// and saves it to `{workspace}/images/{filename}.png`.
pub struct ImageGenTool {
    security: Arc<SecurityPolicy>,
    workspace_dir: PathBuf,
    default_model: String,
    api_key_env: String,
}

impl ImageGenTool {
    pub fn new(
        security: Arc<SecurityPolicy>,
        workspace_dir: PathBuf,
        default_model: String,
        api_key_env: String,
    ) -> Self {
        Self {
            security,
            workspace_dir,
            default_model,
            api_key_env,
        }
    }

    /// Build a reusable HTTP client with reasonable timeouts.
    fn http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default()
    }

    /// Read an API key from the environment.
    fn read_api_key(env_var: &str) -> Result<String, String> {
        std::env::var(env_var)
            .map(|v| v.trim().to_string())
            .ok()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("Missing API key: set the {env_var} environment variable"))
    }

    /// Check if a model identifier targets OpenRouter (provider/model format)
    /// rather than fal.ai (fal-ai/model/variant format).
    fn is_openrouter_model(model: &str) -> bool {
        !model.starts_with("fal-ai/")
    }

    /// Core generation logic: routes to fal.ai or OpenRouter based on model.
    async fn generate(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        // ── Parse parameters ───────────────────────────────────────
        let prompt = match args.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.trim().is_empty() => p.trim().to_string(),
            _ => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Missing required parameter: 'prompt'".into()),
                });
            }
        };

        let filename = args
            .get("filename")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("generated_image");

        // Sanitize filename — strip path components to prevent traversal.
        let safe_name = PathBuf::from(filename).file_name().map_or_else(
            || "generated_image".to_string(),
            |n| n.to_string_lossy().to_string(),
        );

        let model = args
            .get("model")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&self.default_model);

        // Validate model identifier: reject path traversal.
        if model.contains("..") || model.contains('?') || model.contains('#') || model.contains('\\') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Invalid model identifier '{model}'.")),
            });
        }

        // ── Read API key ───────────────────────────────────────────
        let api_key = match Self::read_api_key(&self.api_key_env) {
            Ok(k) => k,
            Err(msg) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(msg),
                });
            }
        };

        // ── Route to backend ───────────────────────────────────────
        let client = Self::http_client();
        let image_url = if Self::is_openrouter_model(model) {
            self.generate_via_openrouter(&client, &api_key, model, &prompt).await?
        } else {
            let size = args.get("size").and_then(|v| v.as_str()).unwrap_or("square_hd");
            const VALID_SIZES: &[&str] = &[
                "square_hd", "landscape_4_3", "portrait_4_3", "landscape_16_9", "portrait_16_9",
            ];
            if !VALID_SIZES.contains(&size) {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Invalid size '{size}'. Valid: {}", VALID_SIZES.join(", "))),
                });
            }
            self.generate_via_fal(&client, &api_key, model, &prompt, size).await?
        };

        // ── Get image bytes (download URL or decode base64 data URI) ──
        let bytes = if image_url.starts_with("data:image") {
            // Base64 data URI: "data:image/png;base64,iVBOR..."
            let b64_data = image_url
                .split_once(",")
                .map(|(_, data)| data)
                .unwrap_or(&image_url);
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(b64_data)
                .context("Failed to decode base64 image data")?
        } else {
            // HTTP URL: download the image
            let img_resp = client
                .get(&image_url)
                .send()
                .await
                .context("Failed to download generated image")?;

            if !img_resp.status().is_success() {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Failed to download image from {} ({})",
                        &image_url[..image_url.len().min(100)],
                        img_resp.status()
                    )),
                });
            }

            img_resp.bytes().await.context("Failed to read image bytes")?.to_vec()
        };

        // ── Save to disk ───────────────────────────────────────────
        let images_dir = self.workspace_dir.join("images");
        tokio::fs::create_dir_all(&images_dir).await.context("Failed to create images directory")?;

        let output_path = images_dir.join(format!("{safe_name}.png"));
        tokio::fs::write(&output_path, &bytes).await.context("Failed to write image file")?;

        let size_kb = bytes.len() / 1024;
        Ok(ToolResult {
            success: true,
            output: format!(
                "Image generated successfully.\nFile: {}\nSize: {} KB\nModel: {}\nPrompt: {}",
                output_path.display(), size_kb, model, prompt,
            ),
            error: None,
        })
    }

    /// Generate via fal.ai synchronous endpoint.
    async fn generate_via_fal(
        &self,
        client: &reqwest::Client,
        api_key: &str,
        model: &str,
        prompt: &str,
        size: &str,
    ) -> anyhow::Result<String> {
        let url = format!("https://fal.run/{model}");
        let body = json!({ "prompt": prompt, "image_size": size, "num_images": 1 });

        let resp = client
            .post(&url)
            .header("Authorization", format!("Key {api_key}"))
            .json(&body)
            .send()
            .await
            .context("fal.ai request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("fal.ai API error ({status}): {body_text}");
        }

        let resp_json: serde_json::Value = resp.json().await.context("Failed to parse fal.ai response")?;
        resp_json
            .pointer("/images/0/url")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("No image URL in fal.ai response"))
    }

    /// Generate via OpenRouter chat completions.
    /// Image-capable models (e.g. `google/gemini-2.5-flash-image`) return
    /// images in `choices[0].message.images[0].image_url.url` as base64 data URIs.
    async fn generate_via_openrouter(
        &self,
        client: &reqwest::Client,
        api_key: &str,
        model: &str,
        prompt: &str,
    ) -> anyhow::Result<String> {
        let url = "https://openrouter.ai/api/v1/chat/completions";
        let body = json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": format!("Generate an image: {prompt}. Only output the image, no text.")
            }]
        });

        let resp = client
            .post(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://zeroclaw.dev")
            .json(&body)
            .send()
            .await
            .context("OpenRouter image gen request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenRouter API error ({status}): {body_text}");
        }

        let resp_json: serde_json::Value = resp.json().await.context("Failed to parse OpenRouter response")?;

        // Format 1: images array (Gemini image models)
        // choices[0].message.images[0].image_url.url = "data:image/png;base64,..."
        if let Some(url) = resp_json
            .pointer("/choices/0/message/images/0/image_url/url")
            .and_then(|v| v.as_str())
        {
            return Ok(url.to_string());
        }

        // Format 2: content array with image_url objects
        if let Some(url) = resp_json
            .pointer("/choices/0/message/content/0/image_url/url")
            .and_then(|v| v.as_str())
        {
            return Ok(url.to_string());
        }

        // Format 3: direct URL string in content
        if let Some(content) = resp_json
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
        {
            if content.starts_with("http") || content.starts_with("data:image") {
                return Ok(content.to_string());
            }
        }

        anyhow::bail!("No image found in OpenRouter response. Check model supports image output.")
    }
}

#[async_trait]
impl Tool for ImageGenTool {
    fn name(&self) -> &str {
        "image_gen"
    }

    fn description(&self) -> &str {
        "Generate an image from a text prompt. Supports OpenRouter models \
         (e.g. bytedance/seedream-4.5) and fal.ai (e.g. fal-ai/flux/schnell). \
         Saves the result to the workspace images directory and returns the file path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text prompt describing the image to generate."
                },
                "filename": {
                    "type": "string",
                    "description": "Output filename without extension (default: 'generated_image'). Saved as PNG in workspace/images/."
                },
                "size": {
                    "type": "string",
                    "enum": ["square_hd", "landscape_4_3", "portrait_4_3", "landscape_16_9", "portrait_16_9"],
                    "description": "Image aspect ratio / size preset (default: 'square_hd')."
                },
                "model": {
                    "type": "string",
                    "description": "fal.ai model identifier (default: 'fal-ai/flux/schnell')."
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        // Security: image generation is a side-effecting action (HTTP + file write).
        if let Err(error) = self
            .security
            .enforce_tool_operation(ToolOperation::Act, "image_gen")
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(error),
            });
        }

        self.generate(args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{AutonomyLevel, SecurityPolicy};

    fn test_security() -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::Full,
            workspace_dir: std::env::temp_dir(),
            ..SecurityPolicy::default()
        })
    }

    fn test_tool() -> ImageGenTool {
        ImageGenTool::new(
            test_security(),
            std::env::temp_dir(),
            "fal-ai/flux/schnell".into(),
            "FAL_API_KEY".into(),
        )
    }

    #[test]
    fn tool_name() {
        let tool = test_tool();
        assert_eq!(tool.name(), "image_gen");
    }

    #[test]
    fn tool_description_is_nonempty() {
        let tool = test_tool();
        assert!(!tool.description().is_empty());
        assert!(tool.description().contains("image"));
    }

    #[test]
    fn tool_schema_has_required_prompt() {
        let tool = test_tool();
        let schema = tool.parameters_schema();
        assert_eq!(schema["required"], json!(["prompt"]));
        assert!(schema["properties"]["prompt"].is_object());
    }

    #[test]
    fn tool_schema_has_optional_params() {
        let tool = test_tool();
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["filename"].is_object());
        assert!(schema["properties"]["size"].is_object());
        assert!(schema["properties"]["model"].is_object());
    }

    #[test]
    fn tool_spec_roundtrip() {
        let tool = test_tool();
        let spec = tool.spec();
        assert_eq!(spec.name, "image_gen");
        assert!(spec.parameters.is_object());
    }

    #[tokio::test]
    async fn missing_prompt_returns_error() {
        let tool = test_tool();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap().contains("prompt"));
    }

    #[tokio::test]
    async fn empty_prompt_returns_error() {
        let tool = test_tool();
        let result = tool.execute(json!({"prompt": "   "})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap().contains("prompt"));
    }

    #[tokio::test]
    async fn missing_api_key_returns_error() {
        // Temporarily ensure the env var is unset.
        let original = std::env::var("FAL_API_KEY_TEST_IMAGE_GEN").ok();
        std::env::remove_var("FAL_API_KEY_TEST_IMAGE_GEN");

        let tool = ImageGenTool::new(
            test_security(),
            std::env::temp_dir(),
            "fal-ai/flux/schnell".into(),
            "FAL_API_KEY_TEST_IMAGE_GEN".into(),
        );
        let result = tool
            .execute(json!({"prompt": "a sunset over the ocean"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .contains("FAL_API_KEY_TEST_IMAGE_GEN"));

        // Restore if it was set.
        if let Some(val) = original {
            std::env::set_var("FAL_API_KEY_TEST_IMAGE_GEN", val);
        }
    }

    #[tokio::test]
    async fn invalid_size_returns_error() {
        // Set a dummy key so we get past the key check.
        std::env::set_var("FAL_API_KEY_TEST_SIZE", "dummy_key");

        let tool = ImageGenTool::new(
            test_security(),
            std::env::temp_dir(),
            "fal-ai/flux/schnell".into(),
            "FAL_API_KEY_TEST_SIZE".into(),
        );
        let result = tool
            .execute(json!({"prompt": "test", "size": "invalid_size"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap().contains("Invalid size"));

        std::env::remove_var("FAL_API_KEY_TEST_SIZE");
    }

    #[tokio::test]
    async fn read_only_autonomy_blocks_execution() {
        let security = Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::ReadOnly,
            workspace_dir: std::env::temp_dir(),
            ..SecurityPolicy::default()
        });
        let tool = ImageGenTool::new(
            security,
            std::env::temp_dir(),
            "fal-ai/flux/schnell".into(),
            "FAL_API_KEY".into(),
        );
        let result = tool.execute(json!({"prompt": "test image"})).await.unwrap();
        assert!(!result.success);
        let err = result.error.as_deref().unwrap();
        assert!(
            err.contains("read-only") || err.contains("image_gen"),
            "expected read-only or image_gen in error, got: {err}"
        );
    }

    #[tokio::test]
    async fn invalid_model_with_traversal_returns_error() {
        std::env::set_var("FAL_API_KEY_TEST_MODEL", "dummy_key");

        let tool = ImageGenTool::new(
            test_security(),
            std::env::temp_dir(),
            "fal-ai/flux/schnell".into(),
            "FAL_API_KEY_TEST_MODEL".into(),
        );
        let result = tool
            .execute(json!({"prompt": "test", "model": "../../evil-endpoint"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .contains("Invalid model identifier"));

        std::env::remove_var("FAL_API_KEY_TEST_MODEL");
    }

    #[test]
    fn read_api_key_missing() {
        let result = ImageGenTool::read_api_key("DEFINITELY_NOT_SET_ZC_TEST_12345");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("DEFINITELY_NOT_SET_ZC_TEST_12345"));
    }

    #[test]
    fn filename_traversal_is_sanitized() {
        // Verify that path traversal in filenames is stripped to just the final component.
        let sanitized = PathBuf::from("../../etc/passwd").file_name().map_or_else(
            || "generated_image".to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        assert_eq!(sanitized, "passwd");

        // ".." alone has no file_name, falls back to default.
        let sanitized = PathBuf::from("..").file_name().map_or_else(
            || "generated_image".to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        assert_eq!(sanitized, "generated_image");
    }

    #[test]
    fn read_api_key_present() {
        std::env::set_var("ZC_IMAGE_GEN_TEST_KEY", "test_value_123");
        let result = ImageGenTool::read_api_key("ZC_IMAGE_GEN_TEST_KEY");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_value_123");
        std::env::remove_var("ZC_IMAGE_GEN_TEST_KEY");
    }
}
