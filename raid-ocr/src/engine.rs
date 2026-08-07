//! Text recognition for prepared raid-frame crops.
//!
//! Band detection supplies the rectangles, so only the recognition model is
//! needed. It is downloaded on first use and loaded once per process.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use rten_imageproc::{Rect, RotatedRect};
use sha2::{Digest, Sha256};

use crate::analysis::PreparedCrop;

const RECOGNITION_MODEL_URLS: &[&str] = &[
    "https://raw.githubusercontent.com/baras-app/baras/master/raid-ocr/model/text-recognition.rten",
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten",
];

const RECOGNITION_MODEL_SHA256: &str =
    "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e";

const MIN_MODEL_BYTES: u64 = 1_000_000;

#[derive(Debug)]
pub enum OcrError {
    ModelUnavailable(String),
    Recognition(String),
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrError::ModelUnavailable(s) => write!(f, "OCR model unavailable: {s}"),
            OcrError::Recognition(s) => write!(f, "Recognition failed: {s}"),
        }
    }
}

impl std::error::Error for OcrError {}

/// Where the downloaded recognition model is cached.
pub fn model_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("models").join("text-recognition.rten"))
}

/// Whether the model has already been downloaded.
pub fn model_is_present() -> bool {
    model_path().is_some_and(|p| p.exists())
}

/// Download the recognition model if it is not already cached.
pub async fn ensure_model() -> Result<PathBuf, OcrError> {
    let path =
        model_path().ok_or_else(|| OcrError::ModelUnavailable("no config directory".into()))?;

    if let Ok(metadata) = std::fs::metadata(&path)
        && metadata.len() >= MIN_MODEL_BYTES
    {
        if engine_is_loaded() || validate_model_file(&path).is_ok() {
            return Ok(path);
        }
        tracing::warn!("Replacing an unreadable OCR model at {path:?}");
    }
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| OcrError::ModelUnavailable(format!("cannot replace {path:?}: {e}")))?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| OcrError::ModelUnavailable(format!("cannot create {parent:?}: {e}")))?;
    }

    tracing::info!("Downloading OCR recognition model to {path:?}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| OcrError::ModelUnavailable(format!("cannot build download client: {e}")))?;

    let mut bytes = None;
    let mut failures = Vec::new();
    for url in RECOGNITION_MODEL_URLS {
        match fetch_model(&client, url).await {
            Ok(fetched) => {
                bytes = Some(fetched);
                break;
            }
            Err(e) => {
                // Log failed mirrors even when a fallback works.
                tracing::warn!("OCR model not available from {url}: {e}");
                failures.push(format!("{url}: {e}"));
            }
        }
    }
    let Some(bytes) = bytes else {
        return Err(OcrError::ModelUnavailable(format!(
            "every source failed ({})",
            failures.join("; ")
        )));
    };

    // Write beside the target and rename, so an interrupted download cannot
    // leave a truncated file that later loads as a corrupt model. The temp name
    // has to keep the `.rten` extension: `Model::load_file` picks its parser
    // from the extension alone, and rejects anything else before reading a byte.
    let temp = path.with_extension("part.rten");
    std::fs::write(&temp, &bytes)
        .map_err(|e| OcrError::ModelUnavailable(format!("cannot write model: {e}")))?;
    if let Err(e) = validate_model_file(&temp) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(OcrError::ModelUnavailable(format!(
            "cannot finalize model: {e}"
        )));
    }

    Ok(path)
}

async fn fetch_model(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let bytes = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    if bytes.len() < MIN_MODEL_BYTES as usize {
        return Err(format!("download was only {} bytes", bytes.len()));
    }

    let digest = hex(&Sha256::digest(&bytes));
    if digest != RECOGNITION_MODEL_SHA256 {
        return Err(format!("served a different model (sha256 {digest})"));
    }

    Ok(bytes.to_vec())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            use std::fmt::Write;
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// Process-wide engine, built once on first use.
static ENGINE: OnceLock<Mutex<Option<Arc<OcrEngine>>>> = OnceLock::new();

fn engine_is_loaded() -> bool {
    ENGINE
        .get()
        .is_some_and(|cell| cell.lock().unwrap_or_else(|p| p.into_inner()).is_some())
}

fn validate_model_file(path: &std::path::Path) -> Result<(), OcrError> {
    Model::load_file(path)
        .map(|_| ())
        .map_err(|e| OcrError::ModelUnavailable(format!("invalid model at {path:?}: {e}")))
}

fn engine() -> Result<Arc<OcrEngine>, OcrError> {
    let cell = ENGINE.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap_or_else(|p| p.into_inner());

    if let Some(existing) = guard.as_ref() {
        return Ok(existing.clone());
    }

    let path =
        model_path().ok_or_else(|| OcrError::ModelUnavailable("no config directory".into()))?;
    if !path.exists() {
        return Err(OcrError::ModelUnavailable(
            "recognition model has not been downloaded".into(),
        ));
    }

    let model = Model::load_file(&path)
        .map_err(|e| OcrError::ModelUnavailable(format!("cannot load {path:?}: {e}")))?;

    let built = OcrEngine::new(OcrEngineParams {
        // No detection model: band detection supplies the rectangles.
        detection_model: None,
        recognition_model: Some(model),
        ..Default::default()
    })
    .map_err(|e| OcrError::ModelUnavailable(format!("cannot build engine: {e}")))?;

    let built = Arc::new(built);
    *guard = Some(built.clone());
    Ok(built)
}

/// Load the engine now, on this thread.
///
/// This holds the engine lock.
/// RTen loads on its own thread pool.
pub fn warm() -> Result<(), OcrError> {
    engine().map(|_| ())
}

/// Recognize the text in one prepared crop.
///
/// The crop is treated as a single text line covering the whole image, since
/// band detection already isolated it.
pub fn recognize(crop: &PreparedCrop) -> Result<String, OcrError> {
    let engine = engine()?;

    let rgb = crop.to_rgb();
    let source = ImageSource::from_bytes(&rgb, (crop.width, crop.height))
        .map_err(|e| OcrError::Recognition(format!("bad image source: {e}")))?;
    let input = engine
        .prepare_input(source)
        .map_err(|e| OcrError::Recognition(format!("prepare_input failed: {e}")))?;

    let line = vec![RotatedRect::from_rect(Rect::from_tlhw(
        0.0,
        0.0,
        crop.height as f32,
        crop.width as f32,
    ))];

    let recognized = engine
        .recognize_text(&input, &[line])
        .map_err(|e| OcrError::Recognition(format!("recognize_text failed: {e}")))?;

    Ok(recognized
        .into_iter()
        .flatten()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string())
}
