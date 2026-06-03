#![forbid(unsafe_code)]

use crate::models::*;
use tracing::instrument;

/// A rich image placeholder descriptor used by the multimodal engine.
///
/// The canonical [`MultimodalConfig`] stores only placeholder *ids*
/// (`Vec<String>`); this type carries the additional metadata the engine
/// needs to validate and render an individual placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImagePlaceholder {
    pub id: String,
    pub description: String,
    pub mime_type: String,
}

/// Multi-modal prompt support engine for image placeholders and MIME validation.
#[derive(Debug, Clone, Default)]
pub struct MultimodalEngine;

/// Supported MIME types for multimodal inputs.
pub const SUPPORTED_IMAGE_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/svg+xml",
    "image/bmp",
    "image/tiff",
];

/// Maximum file size for uploaded images (10 MB).
pub const MAX_IMAGE_SIZE_BYTES: usize = 10 * 1024 * 1024;

impl MultimodalEngine {
    /// Render image placeholders in a template.
    ///
    /// Replaces `{{placeholder_id}}` markers with `[Image: placeholder_id]`
    /// for every placeholder id declared in the config.
    #[instrument]
    pub fn render_placeholders(template: &str, config: &MultimodalConfig) -> String {
        let mut result = template.to_string();

        for id in &config.image_placeholders {
            let marker = format!("{{{{{}}}}}", id);
            let replacement = format!("[Image: {}]", id);
            result = result.replace(&marker, &replacement);
        }

        result
    }

    /// Validate a MIME type for image uploads.
    ///
    /// Returns `true` for supported image formats (PNG, JPEG, GIF, WebP, SVG, BMP, TIFF).
    pub fn validate_mime_type(mime: &str) -> bool {
        SUPPORTED_IMAGE_TYPES.contains(&mime.to_lowercase().as_str())
    }

    /// Validate file size is within limits.
    pub fn validate_file_size(size_bytes: usize) -> bool {
        size_bytes <= MAX_IMAGE_SIZE_BYTES
    }

    /// Full validation for an image placeholder.
    ///
    /// Checks both MIME type and that the placeholder ID is well-formed.
    pub fn validate_placeholder(placeholder: &ImagePlaceholder) -> Result<(), String> {
        if placeholder.id.is_empty() {
            return Err("Placeholder ID cannot be empty".to_string());
        }

        if !Self::validate_mime_type(&placeholder.mime_type) {
            return Err(format!(
                "Unsupported MIME type: {}. Supported: {:?}",
                placeholder.mime_type, SUPPORTED_IMAGE_TYPES
            ));
        }

        if placeholder.description.is_empty() {
            return Err("Placeholder description cannot be empty".to_string());
        }

        Ok(())
    }

    /// Extract placeholder IDs referenced in a template.
    ///
    /// Finds all `{{id}}` patterns and returns the ids.
    pub fn extract_placeholder_ids(template: &str) -> Vec<String> {
        let mut ids = Vec::new();
        // Simple parser for {{id}} patterns
        let mut chars = template.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '{'
                && let Some(&'{') = chars.peek()
            {
                chars.next(); // consume second '{'
                let mut id = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        chars.next(); // consume first '}'
                        if let Some(&'}') = chars.peek() {
                            chars.next(); // consume second '}'
                            if !id.is_empty() {
                                ids.push(id.clone());
                            }
                        }
                        break;
                    } else {
                        id.push(c);
                        chars.next();
                    }
                }
            }
        }
        ids
    }

    /// Check that all placeholders referenced in a template are defined in config.
    pub fn validate_template_references(
        template: &str,
        config: &MultimodalConfig,
    ) -> Result<(), Vec<String>> {
        let referenced = Self::extract_placeholder_ids(template);
        let defined: std::collections::HashSet<_> =
            config.image_placeholders.iter().cloned().collect();

        let missing: Vec<_> = referenced
            .into_iter()
            .filter(|id| !defined.contains(id))
            .collect();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_placeholders() {
        let config = MultimodalConfig {
            supports_images: true,
            image_placeholders: vec!["hero".to_string()],
            ..Default::default()
        };
        let result = MultimodalEngine::render_placeholders("Show {{hero}} here", &config);
        assert!(result.contains("[Image: hero]"));
        assert!(!result.contains("{{hero}}"));
    }

    #[test]
    fn test_render_multiple_placeholders() {
        let config = MultimodalConfig {
            supports_images: true,
            image_placeholders: vec!["header".to_string(), "footer".to_string()],
            ..Default::default()
        };
        let result =
            MultimodalEngine::render_placeholders("{{header}} content {{footer}}", &config);
        assert!(result.contains("[Image: header]"));
        assert!(result.contains("[Image: footer]"));
        assert!(!result.contains("{{header}}"));
        assert!(!result.contains("{{footer}}"));
    }

    #[test]
    fn test_render_no_placeholders() {
        let config = MultimodalConfig::default();
        let template = "Just plain text without any markers.";
        let result = MultimodalEngine::render_placeholders(template, &config);
        assert_eq!(result, template);
    }

    #[test]
    fn test_mime_validation_png() {
        assert!(MultimodalEngine::validate_mime_type("image/png"));
    }

    #[test]
    fn test_mime_validation_jpeg() {
        assert!(MultimodalEngine::validate_mime_type("image/jpeg"));
    }

    #[test]
    fn test_mime_validation_gif() {
        assert!(MultimodalEngine::validate_mime_type("image/gif"));
    }

    #[test]
    fn test_mime_validation_webp() {
        assert!(MultimodalEngine::validate_mime_type("image/webp"));
    }

    #[test]
    fn test_mime_validation_svg() {
        assert!(MultimodalEngine::validate_mime_type("image/svg+xml"));
    }

    #[test]
    fn test_mime_validation_case_insensitive() {
        assert!(MultimodalEngine::validate_mime_type("IMAGE/PNG"));
        assert!(MultimodalEngine::validate_mime_type("Image/Jpeg"));
    }

    #[test]
    fn test_mime_validation_rejects_text() {
        assert!(!MultimodalEngine::validate_mime_type("text/plain"));
    }

    #[test]
    fn test_mime_validation_rejects_video() {
        assert!(!MultimodalEngine::validate_mime_type("video/mp4"));
    }

    #[test]
    fn test_mime_validation_rejects_empty() {
        assert!(!MultimodalEngine::validate_mime_type(""));
    }

    #[test]
    fn test_file_size_validation() {
        assert!(MultimodalEngine::validate_file_size(1024));
        assert!(MultimodalEngine::validate_file_size(MAX_IMAGE_SIZE_BYTES));
        assert!(!MultimodalEngine::validate_file_size(
            MAX_IMAGE_SIZE_BYTES + 1
        ));
    }

    #[test]
    fn test_validate_placeholder_valid() {
        let p = ImagePlaceholder {
            id: "hero".to_string(),
            description: "Hero banner".to_string(),
            mime_type: "image/png".to_string(),
        };
        assert!(MultimodalEngine::validate_placeholder(&p).is_ok());
    }

    #[test]
    fn test_validate_placeholder_empty_id() {
        let p = ImagePlaceholder {
            id: "".to_string(),
            description: "Hero banner".to_string(),
            mime_type: "image/png".to_string(),
        };
        assert!(MultimodalEngine::validate_placeholder(&p).is_err());
    }

    #[test]
    fn test_validate_placeholder_unsupported_mime() {
        let p = ImagePlaceholder {
            id: "video".to_string(),
            description: "A video file".to_string(),
            mime_type: "video/mp4".to_string(),
        };
        assert!(MultimodalEngine::validate_placeholder(&p).is_err());
    }

    #[test]
    fn test_validate_placeholder_empty_description() {
        let p = ImagePlaceholder {
            id: "hero".to_string(),
            description: "".to_string(),
            mime_type: "image/png".to_string(),
        };
        assert!(MultimodalEngine::validate_placeholder(&p).is_err());
    }

    #[test]
    fn test_extract_placeholder_ids() {
        let ids = MultimodalEngine::extract_placeholder_ids("Show {{hero}} and {{footer}} here");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"hero".to_string()));
        assert!(ids.contains(&"footer".to_string()));
    }

    #[test]
    fn test_extract_placeholder_ids_none() {
        let ids = MultimodalEngine::extract_placeholder_ids("No placeholders here.");
        assert!(ids.is_empty());
    }

    #[test]
    fn test_validate_template_references_ok() {
        let config = MultimodalConfig {
            supports_images: true,
            image_placeholders: vec!["hero".to_string()],
            ..Default::default()
        };
        assert!(MultimodalEngine::validate_template_references("Show {{hero}}", &config).is_ok());
    }

    #[test]
    fn test_validate_template_references_missing() {
        let config = MultimodalConfig::default();
        let result = MultimodalEngine::validate_template_references("Show {{hero}}", &config);
        assert!(result.is_err());
        let missing = result.unwrap_err();
        assert!(missing.contains(&"hero".to_string()));
    }
}
