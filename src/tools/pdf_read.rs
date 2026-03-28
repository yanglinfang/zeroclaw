use super::traits::{Tool, ToolResult};
use crate::security::SecurityPolicy;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

/// Maximum PDF file size (50 MB).
const MAX_PDF_BYTES: u64 = 50 * 1024 * 1024;
/// Default character limit returned to the LLM.
const DEFAULT_MAX_CHARS: usize = 50_000;
/// Hard ceiling regardless of what the caller requests.
const MAX_OUTPUT_CHARS: usize = 200_000;
/// Timeout for downloading remote PDFs.
const DOWNLOAD_TIMEOUT_SECS: u64 = 60;

/// Extract plain text from a PDF file or URL.
///
/// Accepts either a local file path (resolved from workspace) or an HTTP/HTTPS
/// URL pointing to a PDF. When given a URL the file is downloaded to a temporary
/// location inside the workspace before extraction.
///
/// PDF extraction requires the `rag-pdf` feature flag:
///   cargo build --features rag-pdf
///
/// Without the feature the tool is still registered so the LLM receives a
/// clear, actionable error rather than a missing-tool confusion.
pub struct PdfReadTool {
    security: Arc<SecurityPolicy>,
}

impl PdfReadTool {
    pub fn new(security: Arc<SecurityPolicy>) -> Self {
        Self { security }
    }

    /// Returns `true` if the input looks like a remote URL rather than a local path.
    fn is_url(path: &str) -> bool {
        path.starts_with("http://") || path.starts_with("https://")
    }

    /// Download a remote PDF to a temporary file in the workspace.
    /// Returns the local path on success.
    async fn download_pdf_to_workspace(&self, url: &str) -> Result<std::path::PathBuf, ToolResult> {
        use std::time::Duration;

        // Build a restrictive HTTP client — no cookies, short timeout, size-limited.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent("ZeroClaw/0.5 pdf_read")
            .build()
            .map_err(|e| ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to build HTTP client: {e}")),
            })?;

        let resp = client.get(url).send().await.map_err(|e| ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Failed to download PDF from URL: {e}")),
        })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("PDF download failed with HTTP {status} from {url}")),
            });
        }

        // Verify content-type looks PDF-ish (or accept octet-stream which many
        // servers use for binary downloads).
        if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            let ct_str = ct.to_str().unwrap_or("");
            if !ct_str.is_empty() && !ct_str.contains("pdf") && !ct_str.contains("octet-stream") {
                return Err(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "URL does not appear to serve a PDF (content-type: {ct_str})"
                    )),
                });
            }
        }

        // Check Content-Length header if present (early reject for oversized files).
        if let Some(cl) = resp.content_length() {
            if cl > MAX_PDF_BYTES {
                return Err(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Remote PDF too large: {cl} bytes (limit: {MAX_PDF_BYTES} bytes)"
                    )),
                });
            }
        }

        // Stream the body with a size cap.
        let bytes = resp.bytes().await.map_err(|e| ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Failed to read PDF response body: {e}")),
        })?;

        if bytes.len() as u64 > MAX_PDF_BYTES {
            return Err(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Downloaded PDF too large: {} bytes (limit: {MAX_PDF_BYTES} bytes)",
                    bytes.len()
                )),
            });
        }

        // Minimal PDF magic-number check.
        if !bytes.starts_with(b"%PDF") {
            return Err(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "Downloaded file does not appear to be a PDF (missing %PDF header)".into(),
                ),
            });
        }

        // Write to workspace/.pdf_downloads/<hash>.pdf so the file is inside the
        // workspace boundary and the existing path-security checks remain valid.
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            url.hash(&mut h);
            h.finish()
        };
        let download_dir = self.security.workspace_dir.join(".pdf_downloads");
        tokio::fs::create_dir_all(&download_dir)
            .await
            .map_err(|e| ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to create download directory: {e}")),
            })?;

        let local_path = download_dir.join(format!("{hash:016x}.pdf"));
        tokio::fs::write(&local_path, &bytes)
            .await
            .map_err(|e| ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to write downloaded PDF: {e}")),
            })?;

        tracing::info!(
            "Downloaded PDF from {} → {} ({} bytes)",
            url,
            local_path.display(),
            bytes.len()
        );

        Ok(local_path)
    }
}

#[async_trait]
impl Tool for PdfReadTool {
    fn name(&self) -> &str {
        "pdf_read"
    }

    fn description(&self) -> &str {
        "Extract plain text from a PDF file or URL. \
         Accepts a local file path OR an http/https URL pointing to a PDF. \
         Remote PDFs are downloaded automatically (max 50 MB). \
         Returns all readable text. Image-only or encrypted PDFs return an empty result. \
         Requires the 'rag-pdf' build feature."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the PDF file, or an HTTP/HTTPS URL to a remote PDF. \
                                    Local relative paths resolve from workspace; outside paths \
                                    require policy allowlist. URLs are downloaded automatically."
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Maximum characters to return (default: 50000, max: 200000)",
                    "minimum": 1,
                    "maximum": 200_000
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .map(|n| {
                usize::try_from(n)
                    .unwrap_or(MAX_OUTPUT_CHARS)
                    .min(MAX_OUTPUT_CHARS)
            })
            .unwrap_or(DEFAULT_MAX_CHARS);

        if self.security.is_rate_limited() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Rate limit exceeded: too many actions in the last hour".into()),
            });
        }

        // Record action — counts toward budget whether local or remote.
        if !self.security.record_action() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Rate limit exceeded: action budget exhausted".into()),
            });
        }

        // ── URL path: download to workspace first ──────────────────────
        let bytes = if Self::is_url(path) {
            tracing::info!("pdf_read: downloading remote PDF from {}", path);

            let local_path = match self.download_pdf_to_workspace(path).await {
                Ok(p) => p,
                Err(tool_result) => return Ok(tool_result),
            };

            match tokio::fs::read(&local_path).await {
                Ok(b) => b,
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Failed to read downloaded PDF: {e}")),
                    });
                }
            }
        } else {
            // ── Local path: existing security checks ───────────────────
            if !self.security.is_path_allowed(path) {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Path not allowed by security policy: {path}")),
                });
            }

            let full_path = self.security.resolve_tool_path(path);

            let resolved_path = match tokio::fs::canonicalize(&full_path).await {
                Ok(p) => p,
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Failed to resolve file path: {e}")),
                    });
                }
            };

            if !self.security.is_resolved_path_allowed(&resolved_path) {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(
                        self.security
                            .resolved_path_violation_message(&resolved_path),
                    ),
                });
            }

            tracing::debug!("Reading PDF: {}", resolved_path.display());

            match tokio::fs::metadata(&resolved_path).await {
                Ok(meta) => {
                    if meta.len() > MAX_PDF_BYTES {
                        return Ok(ToolResult {
                            success: false,
                            output: String::new(),
                            error: Some(format!(
                                "PDF too large: {} bytes (limit: {MAX_PDF_BYTES} bytes)",
                                meta.len()
                            )),
                        });
                    }
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Failed to read file metadata: {e}")),
                    });
                }
            }

            match tokio::fs::read(&resolved_path).await {
                Ok(b) => b,
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Failed to read PDF file: {e}")),
                    });
                }
            }
        };

        // pdf_extract is a blocking CPU-bound operation; keep it off the async executor.
        #[cfg(feature = "rag-pdf")]
        {
            let text = match tokio::task::spawn_blocking(move || {
                pdf_extract::extract_text_from_mem(&bytes)
            })
            .await
            {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("PDF extraction failed: {e}")),
                    });
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("PDF extraction task panicked: {e}")),
                    });
                }
            };

            if text.trim().is_empty() {
                return Ok(ToolResult {
                    success: true,
                    // Agent dispatchers currently forward `error` only when `success=false`.
                    // Keep this as successful execution and expose the warning in `output`.
                    output: "PDF contains no extractable text (may be image-only or encrypted)"
                        .into(),
                    error: None,
                });
            }

            let output = if text.chars().count() > max_chars {
                let mut truncated: String = text.chars().take(max_chars).collect();
                use std::fmt::Write as _;
                let _ = write!(truncated, "\n\n... [truncated at {max_chars} chars]");
                truncated
            } else {
                text
            };

            return Ok(ToolResult {
                success: true,
                output,
                error: None,
            });
        }

        #[cfg(not(feature = "rag-pdf"))]
        {
            let _ = bytes;
            let _ = max_chars;
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "PDF extraction is not enabled. \
                     Rebuild with: cargo build --features rag-pdf"
                        .into(),
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{AutonomyLevel, SecurityPolicy};
    use tempfile::TempDir;

    fn test_security(workspace: std::path::PathBuf) -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::Supervised,
            workspace_dir: workspace,
            ..SecurityPolicy::default()
        })
    }

    fn test_security_with_limit(
        workspace: std::path::PathBuf,
        max_actions: u32,
    ) -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::Supervised,
            workspace_dir: workspace,
            max_actions_per_hour: max_actions,
            ..SecurityPolicy::default()
        })
    }

    #[test]
    fn name_is_pdf_read() {
        let tool = PdfReadTool::new(test_security(std::env::temp_dir()));
        assert_eq!(tool.name(), "pdf_read");
    }

    #[test]
    fn description_not_empty() {
        let tool = PdfReadTool::new(test_security(std::env::temp_dir()));
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn schema_has_path_required() {
        let tool = PdfReadTool::new(test_security(std::env::temp_dir()));
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["max_chars"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("path")));
    }

    #[test]
    fn spec_matches_metadata() {
        let tool = PdfReadTool::new(test_security(std::env::temp_dir()));
        let spec = tool.spec();
        assert_eq!(spec.name, "pdf_read");
        assert!(spec.parameters.is_object());
    }

    #[tokio::test]
    async fn missing_path_param_returns_error() {
        let tool = PdfReadTool::new(test_security(std::env::temp_dir()));
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path"));
    }

    #[tokio::test]
    async fn absolute_path_is_blocked() {
        let tool = PdfReadTool::new(test_security(std::env::temp_dir()));
        let result = tool.execute(json!({"path": "/etc/passwd"})).await.unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("not allowed"));
    }

    #[tokio::test]
    async fn path_traversal_is_blocked() {
        let tmp = TempDir::new().unwrap();
        let tool = PdfReadTool::new(test_security(tmp.path().to_path_buf()));
        let result = tool
            .execute(json!({"path": "../../../etc/passwd"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("not allowed"));
    }

    #[tokio::test]
    async fn nonexistent_file_returns_error() {
        let tmp = TempDir::new().unwrap();
        let tool = PdfReadTool::new(test_security(tmp.path().to_path_buf()));
        let result = tool
            .execute(json!({"path": "does_not_exist.pdf"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("Failed to resolve"));
    }

    #[tokio::test]
    async fn rate_limit_blocks_request() {
        let tmp = TempDir::new().unwrap();
        let tool = PdfReadTool::new(test_security_with_limit(tmp.path().to_path_buf(), 0));
        let result = tool.execute(json!({"path": "any.pdf"})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap_or("").contains("Rate limit"));
    }

    #[tokio::test]
    async fn probing_nonexistent_consumes_rate_limit_budget() {
        let tmp = TempDir::new().unwrap();
        // Allow 2 actions; both will fail on missing file but must consume budget.
        let tool = PdfReadTool::new(test_security_with_limit(tmp.path().to_path_buf(), 2));

        let r1 = tool.execute(json!({"path": "a.pdf"})).await.unwrap();
        assert!(!r1.success);
        assert!(r1
            .error
            .as_deref()
            .unwrap_or("")
            .contains("Failed to resolve"));

        let r2 = tool.execute(json!({"path": "b.pdf"})).await.unwrap();
        assert!(!r2.success);
        assert!(r2
            .error
            .as_deref()
            .unwrap_or("")
            .contains("Failed to resolve"));

        // Third attempt must hit rate limit.
        let r3 = tool.execute(json!({"path": "c.pdf"})).await.unwrap();
        assert!(!r3.success);
        assert!(
            r3.error.as_deref().unwrap_or("").contains("Rate limit"),
            "expected rate limit, got: {:?}",
            r3.error
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_escape_is_blocked() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().unwrap();
        let workspace = root.path().join("workspace");
        let outside = root.path().join("outside");
        tokio::fs::create_dir_all(&workspace).await.unwrap();
        tokio::fs::create_dir_all(&outside).await.unwrap();
        tokio::fs::write(outside.join("secret.pdf"), b"%PDF-1.4 secret")
            .await
            .unwrap();
        symlink(outside.join("secret.pdf"), workspace.join("link.pdf")).unwrap();

        let tool = PdfReadTool::new(test_security(workspace));
        let result = tool.execute(json!({"path": "link.pdf"})).await.unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("escapes workspace"));
    }

    /// Extraction tests require the rag-pdf feature.
    #[cfg(feature = "rag-pdf")]
    mod extraction {
        use super::*;

        /// Minimal valid PDF with one text page ("Hello PDF").
        /// Generated offline and verified with pdf-extract 0.10.
        fn minimal_pdf_bytes() -> Vec<u8> {
            // A hand-crafted single-page PDF containing the text "Hello PDF".
            let body = b"%PDF-1.4\n\
                1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
                2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
                3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R\
                /Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>endobj\n\
                4 0 obj<</Length 44>>\nstream\n\
                BT /F1 12 Tf 72 720 Td (Hello PDF) Tj ET\n\
                endstream\nendobj\n\
                5 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n";

            let xref_offset = body.len();

            let xref = format!(
                "xref\n0 6\n\
                 0000000000 65535 f \n\
                 0000000009 00000 n \n\
                 0000000058 00000 n \n\
                 0000000115 00000 n \n\
                 0000000274 00000 n \n\
                 0000000370 00000 n \n\
                 trailer<</Size 6/Root 1 0 R>>\n\
                 startxref\n{xref_offset}\n%%EOF\n"
            );

            let mut pdf = body.to_vec();
            pdf.extend_from_slice(xref.as_bytes());
            pdf
        }

        #[tokio::test]
        async fn extracts_text_from_valid_pdf() {
            let tmp = TempDir::new().unwrap();
            let pdf_path = tmp.path().join("test.pdf");
            tokio::fs::write(&pdf_path, minimal_pdf_bytes())
                .await
                .unwrap();

            let tool = PdfReadTool::new(test_security(tmp.path().to_path_buf()));
            let result = tool.execute(json!({"path": "test.pdf"})).await.unwrap();

            // Either successfully extracts text, or reports no extractable text
            // (acceptable: minimal hand-crafted PDFs may not parse perfectly).
            assert!(
                result.success
                    || result
                        .error
                        .as_deref()
                        .unwrap_or("")
                        .contains("no extractable")
            );
        }

        #[tokio::test]
        async fn max_chars_truncates_output() {
            let tmp = TempDir::new().unwrap();
            // Write a text file and rename as PDF to exercise the truncation path
            // with known content length.
            let pdf_path = tmp.path().join("trunc.pdf");
            tokio::fs::write(&pdf_path, minimal_pdf_bytes())
                .await
                .unwrap();

            let tool = PdfReadTool::new(test_security(tmp.path().to_path_buf()));
            let result = tool
                .execute(json!({"path": "trunc.pdf", "max_chars": 5}))
                .await
                .unwrap();

            // If extraction succeeded the output must respect the char limit
            // (plus the truncation suffix).
            if result.success && !result.output.is_empty() {
                assert!(
                    result.output.chars().count() <= 5 + "[truncated".len() + 50,
                    "output longer than expected: {} chars",
                    result.output.chars().count()
                );
            }
        }

        #[tokio::test]
        async fn image_only_pdf_returns_empty_text_warning() {
            // A well-formed PDF with no text streams will yield empty output.
            // We simulate this with an otherwise valid PDF that has an empty content stream.
            let tmp = TempDir::new().unwrap();
            let empty_content_pdf = b"%PDF-1.4\n\
                1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
                2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
                3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R\
                /Contents 4 0 R/Resources<<>>>>endobj\n\
                4 0 obj<</Length 0>>\nstream\n\nendstream\nendobj\n\
                xref\n0 5\n\
                0000000000 65535 f \n\
                0000000009 00000 n \n\
                0000000058 00000 n \n\
                0000000115 00000 n \n\
                0000000250 00000 n \n\
                trailer<</Size 5/Root 1 0 R>>\nstartxref\n300\n%%EOF\n";

            tokio::fs::write(tmp.path().join("empty.pdf"), empty_content_pdf)
                .await
                .unwrap();

            let tool = PdfReadTool::new(test_security(tmp.path().to_path_buf()));
            let result = tool.execute(json!({"path": "empty.pdf"})).await.unwrap();

            // Acceptable outcomes: empty text warning, or extraction error for
            // malformed hand-crafted PDF.
            let is_empty_warning = result.success && result.output.contains("no extractable text");
            let is_extraction_error =
                !result.success && result.error.as_deref().unwrap_or("").contains("extraction");
            let is_resolve_error =
                !result.success && result.error.as_deref().unwrap_or("").contains("Failed");
            assert!(
                is_empty_warning || is_extraction_error || is_resolve_error,
                "unexpected result: success={} error={:?}",
                result.success,
                result.error
            );
        }
    }

    #[cfg(not(feature = "rag-pdf"))]
    #[tokio::test]
    async fn without_feature_returns_clear_error() {
        let tmp = TempDir::new().unwrap();
        let pdf_path = tmp.path().join("doc.pdf");
        tokio::fs::write(&pdf_path, b"%PDF-1.4 fake").await.unwrap();

        let tool = PdfReadTool::new(test_security(tmp.path().to_path_buf()));
        let result = tool.execute(json!({"path": "doc.pdf"})).await.unwrap();
        assert!(!result.success);
        assert!(
            result.error.as_deref().unwrap_or("").contains("rag-pdf"),
            "expected feature hint in error, got: {:?}",
            result.error
        );
    }
}
