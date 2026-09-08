use super::{
    comfyui_provider::{
        resolve_local_asset_handle, store_local_asset, store_local_asset_at, StoredAsset,
    },
    domain::{
        LawfulContentCompatibility, MediaCapability, MediaError, MediaErrorCode, MediaKind,
        MediaOutput, MediaProgress, MediaProviderSource, MediaRequest, MediaResult,
    },
    provider::{MediaProvider, MediaProviderMetadata},
};
use crate::{
    provider_selection::{
        AuthorizationKind, AuthorizationRef, AuthorizationState, ProviderInterfaceKind,
    },
    providers::{
        provider_execution_authorization, ProviderConnectionState, ProviderCredentialKind,
        ProviderExecutionAuthorization, ProviderInstance,
    },
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

const XAI_PROVIDER_ID: &str = "grok";
const OPENAI_PROVIDER_ID: &str = "openai";

const XAI_IMAGE_MODEL: &str = "grok-imagine-image-2.0";
const XAI_VIDEO_MODEL: &str = "grok-imagine-video-1.5";
const OPENAI_IMAGE_MODEL: &str = "gpt-image-2";

const XAI_API_BASE: &str = "https://api.x.ai/v1";
const OPENAI_API_BASE: &str = "https://api.openai.com/v1";
const OPENAI_CODEX_BASE: &str = "https://chatgpt.com/backend-api/codex";

const IMAGE_TIMEOUT: Duration = Duration::from_secs(300);
const VIDEO_START_TIMEOUT: Duration = Duration::from_secs(60);
const VIDEO_POLL_TIMEOUT: Duration = Duration::from_secs(300);
const VIDEO_POLL_INTERVAL: Duration = Duration::from_secs(5);
const VIDEO_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloudChargingMode {
    SubscriptionEntitlement,
    MeteredApi,
}

#[derive(Clone)]
struct CloudGeneratedAsset {
    bytes: Vec<u8>,
    mime_type: String,
    model: String,
    request_id: Option<String>,
}

trait CloudMediaBackend: Send + Sync {
    fn execute(
        &self,
        provider_id: &str,
        instance_id: &str,
        request: &MediaRequest,
    ) -> Result<CloudGeneratedAsset, MediaError>;
}

#[derive(Default)]
struct ProductionCloudMediaBackend;

pub(crate) struct CloudMediaProvider {
    metadata: MediaProviderMetadata,
    charging_mode: CloudChargingMode,
    backend: Arc<dyn CloudMediaBackend>,
    asset_root_override: Option<PathBuf>,
}

impl CloudMediaProvider {
    pub(crate) fn from_provider_instance(instance: &ProviderInstance) -> Option<Self> {
        if instance.connection_state != ProviderConnectionState::Connected {
            return None;
        }

        let (capabilities, recommended_cloud, base_priority, compatibility) =
            match instance.provider_id.as_str() {
                XAI_PROVIDER_ID => (
                    vec![
                        MediaCapability::TextToImage,
                        MediaCapability::ImageEdit,
                        MediaCapability::TextToVideo,
                        MediaCapability::ImageToVideo,
                    ],
                    true,
                    200,
                    LawfulContentCompatibility::Broad,
                ),
                OPENAI_PROVIDER_ID => (
                    vec![MediaCapability::TextToImage, MediaCapability::ImageEdit],
                    false,
                    100,
                    LawfulContentCompatibility::Restricted,
                ),
                _ => return None,
            };

        let (authorization_kind, charging_mode, credential_priority) =
            match &instance.credential.kind {
                ProviderCredentialKind::OAuth => (
                    AuthorizationKind::OAuth,
                    CloudChargingMode::SubscriptionEntitlement,
                    20,
                ),
                ProviderCredentialKind::ApiKey => {
                    (AuthorizationKind::ApiKey, CloudChargingMode::MeteredApi, 10)
                }
                ProviderCredentialKind::Local => return None,
            };

        Some(Self {
            metadata: MediaProviderMetadata {
                provider_id: instance.provider_id.clone(),
                provider_instance_id: Some(instance.id.clone()),
                source: MediaProviderSource::Cloud,
                interface_kind: ProviderInterfaceKind::OfficialApi,
                authorization_kind,
                authorization_state: AuthorizationState::Connected,
                authorization_ref: Some(AuthorizationRef::opaque(format!(
                    "provider-instance:{}",
                    instance.id
                ))),
                available: true,
                local_readiness: None,
                priority: base_priority + credential_priority,
                capabilities,
                lawful_content_compatibility: compatibility,
                recommended_cloud,
                supports_progress: true,
                supports_cancellation: false,
                supports_cost_estimation: false,
            },
            charging_mode,
            backend: Arc::new(ProductionCloudMediaBackend),
            asset_root_override: None,
        })
    }

    fn validate_request(&self, request: &MediaRequest) -> Result<(), MediaError> {
        if !self.metadata.supports(request.capability) {
            return Err(media_error(
                MediaErrorCode::UnsupportedCapability,
                "The selected Cloud Media provider does not support this capability.",
                false,
            ));
        }

        if request.intent.request.trim().is_empty() {
            return Err(media_error(
                MediaErrorCode::InvalidRequest,
                "The Generative Media request is empty.",
                false,
            ));
        }

        let expected_kind = match request.capability {
            MediaCapability::TextToImage | MediaCapability::ImageEdit => MediaKind::Image,
            MediaCapability::TextToVideo | MediaCapability::ImageToVideo => MediaKind::Video,
            _ => {
                return Err(media_error(
                    MediaErrorCode::UnsupportedCapability,
                    "This Cloud Media capability is not implemented in GM-4.",
                    false,
                ))
            }
        };

        if request.intent.media_kind != expected_kind {
            return Err(media_error(
                MediaErrorCode::InvalidRequest,
                "CreativeIntent media kind does not match the requested capability.",
                false,
            ));
        }

        if matches!(
            request.capability,
            MediaCapability::ImageEdit | MediaCapability::ImageToVideo
        ) && request
            .intent
            .references
            .iter()
            .all(|reference| reference.kind != MediaKind::Image)
        {
            return Err(media_error(
                MediaErrorCode::InvalidRequest,
                "This Cloud Media operation requires an image reference.",
                false,
            ));
        }

        Ok(())
    }

    fn admit_spend(&self) -> Result<(), MediaError> {
        match self.charging_mode {
            CloudChargingMode::SubscriptionEntitlement => Ok(()),
            CloudChargingMode::MeteredApi => Err(media_error(
                MediaErrorCode::BudgetExceeded,
                "Metered API Cloud Media execution is fail-closed until an explicit AI-OS media spend envelope has been authorized. No Provider credential was read and no network request was made.",
                false,
            )),
        }
    }

    fn persist_asset(&self, generated: &CloudGeneratedAsset) -> Result<StoredAsset, MediaError> {
        match self.asset_root_override.as_ref() {
            Some(root) => store_local_asset_at(root, &generated.bytes, &generated.mime_type),
            None => store_local_asset(&generated.bytes, &generated.mime_type),
        }
        .map_err(|message| media_error(MediaErrorCode::ProviderError, &message, true))
    }

    #[cfg(test)]
    fn fixture(
        provider_id: &str,
        instance_id: &str,
        charging_mode: CloudChargingMode,
        backend: Arc<dyn CloudMediaBackend>,
        asset_root: PathBuf,
    ) -> Self {
        let capabilities = if provider_id == XAI_PROVIDER_ID {
            vec![
                MediaCapability::TextToImage,
                MediaCapability::ImageEdit,
                MediaCapability::TextToVideo,
                MediaCapability::ImageToVideo,
            ]
        } else {
            vec![MediaCapability::TextToImage, MediaCapability::ImageEdit]
        };

        Self {
            metadata: MediaProviderMetadata {
                provider_id: provider_id.to_owned(),
                provider_instance_id: Some(instance_id.to_owned()),
                source: MediaProviderSource::Cloud,
                interface_kind: ProviderInterfaceKind::OfficialApi,
                authorization_kind: match charging_mode {
                    CloudChargingMode::SubscriptionEntitlement => AuthorizationKind::OAuth,
                    CloudChargingMode::MeteredApi => AuthorizationKind::ApiKey,
                },
                authorization_state: AuthorizationState::Connected,
                authorization_ref: Some(AuthorizationRef::opaque(format!(
                    "provider-instance:{instance_id}"
                ))),
                available: true,
                local_readiness: None,
                priority: if provider_id == XAI_PROVIDER_ID {
                    220
                } else {
                    120
                },
                capabilities,
                lawful_content_compatibility: LawfulContentCompatibility::Unknown,
                recommended_cloud: provider_id == XAI_PROVIDER_ID,
                supports_progress: true,
                supports_cancellation: false,
                supports_cost_estimation: false,
            },
            charging_mode,
            backend,
            asset_root_override: Some(asset_root),
        }
    }
}

impl MediaProvider for CloudMediaProvider {
    fn metadata(&self) -> &MediaProviderMetadata {
        &self.metadata
    }

    fn execute(
        &self,
        request: &MediaRequest,
        report: &mut dyn FnMut(MediaProgress),
    ) -> Result<MediaResult, MediaError> {
        self.validate_request(request)?;

        // Metered API requests stop here, before the production backend can
        // resolve P13 credentials or contact a provider.
        self.admit_spend()?;

        report(MediaProgress {
            phase: "starting".to_owned(),
            completed_units: Some(0),
            total_units: Some(3),
            message: "Starting the selected Cloud Media provider.".to_owned(),
        });

        let instance_id = self
            .metadata
            .provider_instance_id
            .as_deref()
            .ok_or_else(|| {
                media_error(
                    MediaErrorCode::Authentication,
                    "Cloud Media Provider Instance is missing.",
                    false,
                )
            })?;

        report(MediaProgress {
            phase: "generating".to_owned(),
            completed_units: Some(1),
            total_units: Some(3),
            message: "Generating media with the selected connected cloud account.".to_owned(),
        });

        let generated = self
            .backend
            .execute(&self.metadata.provider_id, instance_id, request)?;

        if generated.bytes.is_empty() {
            return Err(media_error(
                MediaErrorCode::ProviderError,
                "Cloud Media provider returned an empty asset.",
                false,
            ));
        }

        let stored = self.persist_asset(&generated)?;

        let digest = {
            let mut hasher = Sha256::new();
            hasher.update(&generated.bytes);
            format!("{:x}", hasher.finalize())
        };

        report(MediaProgress {
            phase: "completed".to_owned(),
            completed_units: Some(3),
            total_units: Some(3),
            message: "Cloud media generation completed.".to_owned(),
        });

        Ok(MediaResult {
            provider_id: self.metadata.provider_id.clone(),
            provider_instance_id: self.metadata.provider_instance_id.clone(),
            outputs: vec![MediaOutput {
                id: stored.asset_id.clone(),
                kind: request.intent.media_kind,
                mime_type: generated.mime_type.clone(),
                handle: stored.handle.clone(),
            }],
            metadata: serde_json::json!({
                "source": "cloud",
                "model": generated.model,
                "chargingMode": match self.charging_mode {
                    CloudChargingMode::SubscriptionEntitlement => "subscription-entitlement",
                    CloudChargingMode::MeteredApi => "metered-api",
                },
                "requestId": generated.request_id,
                "outputBytes": generated.bytes.len(),
                "outputSha256": digest,
                "assetId": stored.asset_id,
            }),
        })
    }
}

impl CloudMediaBackend for ProductionCloudMediaBackend {
    fn execute(
        &self,
        provider_id: &str,
        instance_id: &str,
        request: &MediaRequest,
    ) -> Result<CloudGeneratedAsset, MediaError> {
        let authorization = tauri::async_runtime::block_on(provider_execution_authorization(
            instance_id,
            "generative_media_cloud",
        ))
        .map_err(|message| media_error(MediaErrorCode::Authentication, &message, false))?;

        match provider_id {
            XAI_PROVIDER_ID => execute_xai(&authorization, request),
            OPENAI_PROVIDER_ID => execute_openai(&authorization, request),
            _ => Err(media_error(
                MediaErrorCode::ProviderUnavailable,
                "The selected Cloud Media provider is not implemented.",
                false,
            )),
        }
    }
}

fn execute_xai(
    authorization: &ProviderExecutionAuthorization,
    request: &MediaRequest,
) -> Result<CloudGeneratedAsset, MediaError> {
    match request.capability {
        MediaCapability::TextToImage | MediaCapability::ImageEdit => {
            execute_xai_image(authorization, request)
        }
        MediaCapability::TextToVideo | MediaCapability::ImageToVideo => {
            execute_xai_video(authorization, request)
        }
        _ => Err(media_error(
            MediaErrorCode::UnsupportedCapability,
            "xAI Cloud Media capability is unsupported.",
            false,
        )),
    }
}

fn execute_xai_image(
    authorization: &ProviderExecutionAuthorization,
    request: &MediaRequest,
) -> Result<CloudGeneratedAsset, MediaError> {
    let client = blocking_client(IMAGE_TIMEOUT)?;
    let session_id = Uuid::new_v4().to_string();

    let (url, payload) = match request.capability {
        MediaCapability::TextToImage => (
            format!("{XAI_API_BASE}/images/generations"),
            serde_json::json!({
                "model": XAI_IMAGE_MODEL,
                "prompt": request.intent.request,
                "n": 1,
                "aspect_ratio": "1:1",
                "resolution": "1k",
                "response_format": "b64_json"
            }),
        ),
        MediaCapability::ImageEdit => {
            let images = image_reference_data_urls(request)?;
            let mut payload = serde_json::json!({
                "model": XAI_IMAGE_MODEL,
                "prompt": request.intent.request,
                "n": 1,
                "resolution": "1k",
                "response_format": "b64_json"
            });

            if images.len() == 1 {
                payload["image"] = serde_json::json!({ "url": images[0] });
            } else {
                payload["images"] = serde_json::Value::Array(
                    images
                        .into_iter()
                        .map(|url| serde_json::json!({ "url": url }))
                        .collect(),
                );
                payload["aspect_ratio"] = serde_json::json!("auto");
            }

            (format!("{XAI_API_BASE}/images/edits"), payload)
        }
        _ => {
            return Err(media_error(
                MediaErrorCode::UnsupportedCapability,
                "xAI image execution received a non-image capability.",
                false,
            ))
        }
    };

    let response = client
        .post(url)
        .bearer_auth(&authorization.access_token)
        .header("x-grok-session-id", session_id)
        .json(&payload)
        .send()
        .map_err(|_| {
            media_error(
                MediaErrorCode::ProviderUnavailable,
                "xAI Imagine could not be reached.",
                true,
            )
        })?;

    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let body = checked_response_text(response, "xAI Imagine")?;
    let bytes = decode_image_response(&body)?;

    Ok(CloudGeneratedAsset {
        mime_type: detect_image_mime(&bytes)?.to_owned(),
        bytes,
        model: XAI_IMAGE_MODEL.to_owned(),
        request_id,
    })
}

fn execute_xai_video(
    authorization: &ProviderExecutionAuthorization,
    request: &MediaRequest,
) -> Result<CloudGeneratedAsset, MediaError> {
    let client = blocking_client(VIDEO_START_TIMEOUT)?;
    let session_id = Uuid::new_v4().to_string();

    let image = if request.capability == MediaCapability::ImageToVideo {
        Some(
            image_reference_data_urls(request)?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    media_error(
                        MediaErrorCode::InvalidRequest,
                        "Image-to-video requires an image reference.",
                        false,
                    )
                })?,
        )
    } else {
        None
    };

    let mut payload = serde_json::json!({
        "model": XAI_VIDEO_MODEL,
        "prompt": request.intent.request,
        "duration": 6,
        "aspect_ratio": "16:9",
        "resolution": "480p"
    });

    if let Some(image) = image {
        payload["image"] = serde_json::json!({ "url": image });
    }

    let response = client
        .post(format!("{XAI_API_BASE}/videos/generations"))
        .bearer_auth(&authorization.access_token)
        .header("x-grok-session-id", &session_id)
        .json(&payload)
        .send()
        .map_err(|_| {
            media_error(
                MediaErrorCode::ProviderUnavailable,
                "xAI video generation could not be started.",
                true,
            )
        })?;

    let body = checked_response_text(response, "xAI video generation")?;
    let start: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
        media_error(
            MediaErrorCode::ProviderError,
            "xAI returned an invalid video start response.",
            false,
        )
    })?;

    let request_id = start
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            media_error(
                MediaErrorCode::ProviderError,
                "xAI video generation did not return a request identifier.",
                false,
            )
        })?
        .to_owned();

    let poll_client = blocking_client(Duration::from_secs(30))?;
    let started = Instant::now();

    while started.elapsed() < VIDEO_POLL_TIMEOUT {
        thread::sleep(VIDEO_POLL_INTERVAL);

        let response = poll_client
            .get(format!("{XAI_API_BASE}/videos/{request_id}"))
            .bearer_auth(&authorization.access_token)
            .header("x-grok-session-id", &session_id)
            .send()
            .map_err(|_| {
                media_error(
                    MediaErrorCode::ProviderUnavailable,
                    "xAI video generation status is temporarily unavailable.",
                    true,
                )
            })?;

        let body = checked_response_text(response, "xAI video generation status")?;
        let poll: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
            media_error(
                MediaErrorCode::ProviderError,
                "xAI returned an invalid video status response.",
                false,
            )
        })?;

        match poll
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
        {
            "done" => {
                let url = poll
                    .pointer("/video/url")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        media_error(
                            MediaErrorCode::ProviderError,
                            "xAI video generation completed without a download URL.",
                            false,
                        )
                    })?;

                let download_client = blocking_client(VIDEO_DOWNLOAD_TIMEOUT)?;
                let response = download_client.get(url).send().map_err(|_| {
                    media_error(
                        MediaErrorCode::ProviderUnavailable,
                        "Generated xAI video could not be downloaded.",
                        true,
                    )
                })?;

                if !response.status().is_success() {
                    return Err(http_status_error(
                        response.status(),
                        "Generated xAI video download",
                    ));
                }

                let bytes = response.bytes().map_err(|_| {
                    media_error(
                        MediaErrorCode::ProviderError,
                        "Generated xAI video could not be read.",
                        true,
                    )
                })?;

                return Ok(CloudGeneratedAsset {
                    bytes: bytes.to_vec(),
                    mime_type: "video/mp4".to_owned(),
                    model: XAI_VIDEO_MODEL.to_owned(),
                    request_id: Some(request_id),
                });
            }
            "failed" => {
                return Err(media_error(
                    MediaErrorCode::ProviderError,
                    "xAI video generation failed.",
                    false,
                ))
            }
            _ => {}
        }
    }

    Err(media_error(
        MediaErrorCode::ProviderUnavailable,
        "xAI video generation did not complete before the GM-4 timeout.",
        true,
    ))
}

fn execute_openai(
    authorization: &ProviderExecutionAuthorization,
    request: &MediaRequest,
) -> Result<CloudGeneratedAsset, MediaError> {
    if !matches!(
        request.capability,
        MediaCapability::TextToImage | MediaCapability::ImageEdit
    ) {
        return Err(media_error(
            MediaErrorCode::UnsupportedCapability,
            "OpenAI Cloud Media currently supports image generation and image editing.",
            false,
        ));
    }

    let uses_codex = authorization.route_kind.as_deref() == Some("openai-codex");
    let base = if uses_codex {
        OPENAI_CODEX_BASE
    } else {
        OPENAI_API_BASE
    };

    let (path, payload) = match request.capability {
        MediaCapability::TextToImage => (
            "images/generations",
            serde_json::json!({
                "model": OPENAI_IMAGE_MODEL,
                "prompt": request.intent.request,
                "quality": "low",
                "size": "1024x1024"
            }),
        ),
        MediaCapability::ImageEdit => (
            "images/edits",
            serde_json::json!({
                "model": OPENAI_IMAGE_MODEL,
                "prompt": request.intent.request,
                "images": image_reference_data_urls(request)?
                    .into_iter()
                    .map(|image_url| serde_json::json!({ "image_url": image_url }))
                    .collect::<Vec<_>>(),
                "quality": "low",
                "size": "1024x1024"
            }),
        ),
        _ => unreachable!(),
    };

    let client = blocking_client(IMAGE_TIMEOUT)?;
    let mut builder = client
        .post(format!("{}/{}", base.trim_end_matches('/'), path))
        .bearer_auth(&authorization.access_token);

    if uses_codex {
        builder = builder
            .header("OpenAI-Beta", "codex-1")
            .header("originator", "ai-os")
            .header("x-codex-image-turn-id", Uuid::new_v4().to_string());

        if let Some(account_id) = authorization.account_id.as_deref() {
            builder = builder.header("ChatGPT-Account-ID", account_id);
        }
    }

    let response = builder.json(&payload).send().map_err(|_| {
        media_error(
            MediaErrorCode::ProviderUnavailable,
            "OpenAI image generation could not be reached.",
            true,
        )
    })?;

    let request_id = response
        .headers()
        .get("x-codex-imagegen-request-id")
        .or_else(|| response.headers().get("x-request-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let body = checked_response_text(response, "OpenAI image generation")?;
    let bytes = decode_image_response(&body)?;

    Ok(CloudGeneratedAsset {
        mime_type: detect_image_mime(&bytes)?.to_owned(),
        bytes,
        model: OPENAI_IMAGE_MODEL.to_owned(),
        request_id,
    })
}

fn blocking_client(timeout: Duration) -> Result<reqwest::blocking::Client, MediaError> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|_| {
            media_error(
                MediaErrorCode::ProviderError,
                "Cloud Media HTTP client could not be initialized.",
                false,
            )
        })
}

fn checked_response_text(
    response: reqwest::blocking::Response,
    provider: &str,
) -> Result<String, MediaError> {
    let status = response.status();

    if !status.is_success() {
        return Err(http_status_error(status, provider));
    }

    response.text().map_err(|_| {
        media_error(
            MediaErrorCode::ProviderError,
            &format!("{provider} returned an unreadable response."),
            true,
        )
    })
}

fn http_status_error(status: reqwest::StatusCode, provider: &str) -> MediaError {
    let (code, retryable, message) = match status.as_u16() {
        401 => (
            MediaErrorCode::Authentication,
            false,
            format!("{provider} rejected the connected account authorization."),
        ),
        402 => (
            MediaErrorCode::BudgetExceeded,
            false,
            format!("{provider} requires additional billing authorization."),
        ),
        403 => (
            MediaErrorCode::ProviderUnavailable,
            false,
            format!(
                "{provider} is connected, but this account is not entitled to this media capability."
            ),
        ),
        429 => (
            MediaErrorCode::ProviderUnavailable,
            true,
            format!("{provider} is temporarily rate limited."),
        ),
        value if value >= 500 => (
            MediaErrorCode::ProviderError,
            true,
            format!("{provider} is temporarily unavailable (HTTP {value})."),
        ),
        value => (
            MediaErrorCode::ProviderError,
            false,
            format!("{provider} rejected the media request (HTTP {value})."),
        ),
    };

    media_error(code, &message, retryable)
}

fn decode_image_response(body: &str) -> Result<Vec<u8>, MediaError> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| {
        media_error(
            MediaErrorCode::ProviderError,
            "Cloud image provider returned invalid JSON.",
            false,
        )
    })?;

    let encoded = value
        .pointer("/data/0/b64_json")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            media_error(
                MediaErrorCode::ProviderError,
                "Cloud image provider returned no image data.",
                false,
            )
        })?;

    BASE64_STANDARD.decode(encoded).map_err(|_| {
        media_error(
            MediaErrorCode::ProviderError,
            "Cloud image provider returned invalid image data.",
            false,
        )
    })
}

fn image_reference_data_urls(request: &MediaRequest) -> Result<Vec<String>, MediaError> {
    let mut values = Vec::new();

    for reference in request
        .intent
        .references
        .iter()
        .filter(|reference| reference.kind == MediaKind::Image)
    {
        let handle = reference.handle.trim();

        if handle.starts_with("data:image/") {
            if !handle.contains(";base64,") {
                return Err(media_error(
                    MediaErrorCode::InvalidRequest,
                    "Image reference data URLs must use base64 encoding.",
                    false,
                ));
            }

            values.push(handle.to_owned());
            continue;
        }

        let path = resolve_local_asset_handle(handle).map_err(|_| {
            media_error(
                MediaErrorCode::InvalidRequest,
                "GM-4 can resolve image references from AI-OS asset handles or base64 data URLs only.",
                false,
            )
        })?;

        let bytes = fs::read(path).map_err(|_| {
            media_error(
                MediaErrorCode::InvalidRequest,
                "The referenced AI-OS image asset is unavailable.",
                false,
            )
        })?;

        let mime = detect_image_mime(&bytes)?;
        values.push(format!(
            "data:{mime};base64,{}",
            BASE64_STANDARD.encode(bytes)
        ));
    }

    if values.is_empty() {
        return Err(media_error(
            MediaErrorCode::InvalidRequest,
            "No usable image reference was supplied.",
            false,
        ));
    }

    Ok(values)
}

fn detect_image_mime(bytes: &[u8]) -> Result<&'static str, MediaError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok("image/png");
    }

    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Ok("image/jpeg");
    }

    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok("image/webp");
    }

    Err(media_error(
        MediaErrorCode::ProviderError,
        "Cloud provider returned an unsupported image format.",
        false,
    ))
}

fn media_error(code: MediaErrorCode, message: &str, retryable: bool) -> MediaError {
    MediaError {
        code,
        message: message.to_owned(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generative_media::domain::{
        CreativeIntent, MediaExecutionTarget, MediaProviderSelection, MediaReference,
        MediaRouteMode,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockBackend {
        calls: Arc<AtomicUsize>,
        outcome: Result<CloudGeneratedAsset, MediaError>,
    }

    impl CloudMediaBackend for MockBackend {
        fn execute(
            &self,
            _provider_id: &str,
            _instance_id: &str,
            _request: &MediaRequest,
        ) -> Result<CloudGeneratedAsset, MediaError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.outcome.clone()
        }
    }

    fn request(
        capability: MediaCapability,
        kind: MediaKind,
        provider: &str,
        instance: &str,
    ) -> MediaRequest {
        MediaRequest {
            capability,
            intent: CreativeIntent {
                request: "Create a minimal product image".to_owned(),
                media_kind: kind,
                references: Vec::new(),
                constraints: Vec::new(),
                preferences: Vec::new(),
            },
            route_mode: MediaRouteMode::Manual,
            execution_target: MediaExecutionTarget::Cloud,
            manual_provider: Some(MediaProviderSelection {
                provider_id: provider.to_owned(),
                provider_instance_id: Some(instance.to_owned()),
            }),
        }
    }

    fn png_outcome() -> CloudGeneratedAsset {
        CloudGeneratedAsset {
            bytes: b"\x89PNG\r\n\x1a\ncloud-fixture".to_vec(),
            mime_type: "image/png".to_owned(),
            model: "fixture-model".to_owned(),
            request_id: Some("request-fixture".to_owned()),
        }
    }

    #[test]
    fn oauth_subscription_executes_and_returns_opaque_ai_os_asset() {
        let root = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let backend = Arc::new(MockBackend {
            calls: Arc::clone(&calls),
            outcome: Ok(png_outcome()),
        });

        let provider = CloudMediaProvider::fixture(
            XAI_PROVIDER_ID,
            "grok-default",
            CloudChargingMode::SubscriptionEntitlement,
            backend,
            root.path().to_path_buf(),
        );

        let result = provider
            .execute(
                &request(
                    MediaCapability::TextToImage,
                    MediaKind::Image,
                    XAI_PROVIDER_ID,
                    "grok-default",
                ),
                &mut |_| {},
            )
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.provider_id, XAI_PROVIDER_ID);
        assert_eq!(result.provider_instance_id.as_deref(), Some("grok-default"));
        assert!(result.outputs[0]
            .handle
            .starts_with("asset://generative-media/"));
        assert!(!result.outputs[0]
            .handle
            .contains(root.path().to_string_lossy().as_ref()));

        let serialized = serde_json::to_string(&result).unwrap();

        for forbidden in [
            "accessToken",
            "refreshToken",
            "apiKey",
            "Bearer ",
            "ChatGPT-Account-ID",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn metered_api_key_fails_closed_before_backend_execution() {
        let root = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let backend = Arc::new(MockBackend {
            calls: Arc::clone(&calls),
            outcome: Ok(png_outcome()),
        });

        let provider = CloudMediaProvider::fixture(
            XAI_PROVIDER_ID,
            "grok-api",
            CloudChargingMode::MeteredApi,
            backend,
            root.path().to_path_buf(),
        );

        let error = provider
            .execute(
                &request(
                    MediaCapability::TextToImage,
                    MediaKind::Image,
                    XAI_PROVIDER_ID,
                    "grok-api",
                ),
                &mut |_| {},
            )
            .unwrap_err();

        assert_eq!(error.code, MediaErrorCode::BudgetExceeded);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(root.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn unsupported_cloud_capability_stops_before_backend() {
        let root = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let backend = Arc::new(MockBackend {
            calls: Arc::clone(&calls),
            outcome: Ok(png_outcome()),
        });

        let provider = CloudMediaProvider::fixture(
            OPENAI_PROVIDER_ID,
            "openai-default",
            CloudChargingMode::SubscriptionEntitlement,
            backend,
            root.path().to_path_buf(),
        );

        let error = provider
            .execute(
                &request(
                    MediaCapability::TextToVideo,
                    MediaKind::Video,
                    OPENAI_PROVIDER_ID,
                    "openai-default",
                ),
                &mut |_| {},
            )
            .unwrap_err();

        assert_eq!(error.code, MediaErrorCode::UnsupportedCapability);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn image_edit_requires_reference_before_backend() {
        let root = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let backend = Arc::new(MockBackend {
            calls: Arc::clone(&calls),
            outcome: Ok(png_outcome()),
        });

        let provider = CloudMediaProvider::fixture(
            XAI_PROVIDER_ID,
            "grok-default",
            CloudChargingMode::SubscriptionEntitlement,
            backend,
            root.path().to_path_buf(),
        );

        let error = provider
            .execute(
                &request(
                    MediaCapability::ImageEdit,
                    MediaKind::Image,
                    XAI_PROVIDER_ID,
                    "grok-default",
                ),
                &mut |_| {},
            )
            .unwrap_err();

        assert_eq!(error.code, MediaErrorCode::InvalidRequest);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn managed_asset_reference_can_be_encoded_for_cloud_provider() {
        let root = tempfile::tempdir().unwrap();
        let stored =
            store_local_asset_at(root.path(), b"\x89PNG\r\n\x1a\nreference", "image/png").unwrap();

        // Production handle resolution intentionally resolves the real AI-OS
        // root, so this test verifies data URLs separately from the test root.
        let request = MediaRequest {
            capability: MediaCapability::ImageEdit,
            intent: CreativeIntent {
                request: "Edit this image".to_owned(),
                media_kind: MediaKind::Image,
                references: vec![MediaReference {
                    id: "reference".to_owned(),
                    kind: MediaKind::Image,
                    handle: format!(
                        "data:image/png;base64,{}",
                        BASE64_STANDARD.encode(b"\x89PNG\r\n\x1a\nreference")
                    ),
                }],
                constraints: Vec::new(),
                preferences: Vec::new(),
            },
            route_mode: MediaRouteMode::Manual,
            execution_target: MediaExecutionTarget::Cloud,
            manual_provider: Some(MediaProviderSelection {
                provider_id: XAI_PROVIDER_ID.to_owned(),
                provider_instance_id: Some("grok-default".to_owned()),
            }),
        };

        let encoded = image_reference_data_urls(&request).unwrap();

        assert_eq!(encoded.len(), 1);
        assert!(encoded[0].starts_with("data:image/png;base64,"));
        assert!(stored.handle.starts_with("asset://generative-media/"));
    }

    #[test]
    fn xai_is_recommended_cloud_and_openai_remains_secondary() {
        let root = tempfile::tempdir().unwrap();
        let backend: Arc<dyn CloudMediaBackend> = Arc::new(MockBackend {
            calls: Arc::new(AtomicUsize::new(0)),
            outcome: Ok(png_outcome()),
        });

        let xai = CloudMediaProvider::fixture(
            XAI_PROVIDER_ID,
            "grok-default",
            CloudChargingMode::SubscriptionEntitlement,
            Arc::clone(&backend),
            root.path().to_path_buf(),
        );

        let openai = CloudMediaProvider::fixture(
            OPENAI_PROVIDER_ID,
            "openai-default",
            CloudChargingMode::SubscriptionEntitlement,
            backend,
            root.path().to_path_buf(),
        );

        assert!(xai.metadata().recommended_cloud);
        assert!(!openai.metadata().recommended_cloud);
        assert!(xai.metadata().priority > openai.metadata().priority);
    }

    #[test]
    #[ignore = "requires explicit opt-in and a real connected cloud entitlement"]
    fn live_connected_grok_oauth_text_to_image() {
        let instance = crate::providers::list_provider_instances()
            .unwrap()
            .into_iter()
            .find(|instance| {
                instance.provider_id == XAI_PROVIDER_ID
                    && instance.connection_state == ProviderConnectionState::Connected
                    && instance.credential.kind == ProviderCredentialKind::OAuth
            })
            .expect("connected Grok OAuth Provider Instance required");

        let provider = CloudMediaProvider::from_provider_instance(&instance)
            .expect("connected Grok OAuth instance should become a Cloud Media provider");

        let result = provider
            .execute(
                &request(
                    MediaCapability::TextToImage,
                    MediaKind::Image,
                    XAI_PROVIDER_ID,
                    &instance.id,
                ),
                &mut |progress| {
                    eprintln!("LIVE_GM4_PROGRESS {} {}", progress.phase, progress.message)
                },
            )
            .expect("connected Grok OAuth media execution should succeed");

        let output = result.outputs.first().expect("image output required");

        eprintln!(
            "LIVE_GM4 provider={} instance={} mime={} handle={}",
            result.provider_id,
            result.provider_instance_id.as_deref().unwrap_or_default(),
            output.mime_type,
            output.handle
        );

        assert!(output.handle.starts_with("asset://generative-media/"));
    }
}
