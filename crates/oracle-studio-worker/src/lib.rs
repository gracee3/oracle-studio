//! Worker actor and the sole production [`oracle_studio_platform::StudioPlatform`] implementation.

#![cfg(target_arch = "wasm32")]

use std::{cell::RefCell, rc::Rc};

use futures::{channel::oneshot, lock::Mutex};
use gloo_worker::{Codec, HandlerId, Spawnable, Worker, WorkerBridge, WorkerScope};
use oracle_studio_browser::{BrowserStudioEngine, IndexedDbStore};
use oracle_studio_location_catalog::{CatalogInstallInput, CatalogRetrieval};
use oracle_studio_platform::{
    PlatformCommand, PlatformError, PlatformErrorCode, PlatformFuture, PlatformResponse,
    StudioPlatform,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Response, WorkerGlobalScope};

pub struct StudioWorker {
    engine: Rc<Mutex<Option<BrowserStudioEngine>>>,
}

/// JSON keeps Serde's tagged domain enums intact across the worker boundary.
///
/// The default gloo-worker bincode codec cannot decode internally tagged enums,
/// which are part of the canonical vault and catalog models.
pub struct StudioCodec;

impl Codec for StudioCodec {
    fn encode<I>(input: I) -> JsValue
    where
        I: serde::Serialize,
    {
        let bytes = serde_json::to_vec(&input).expect("worker message must serialize");
        js_sys::Uint8Array::from(bytes.as_slice()).into()
    }

    fn decode<O>(input: JsValue) -> O
    where
        O: for<'de> serde::Deserialize<'de>,
    {
        let bytes = js_sys::Uint8Array::from(input).to_vec();
        serde_json::from_slice(&bytes).expect("worker message must deserialize")
    }
}

impl Worker for StudioWorker {
    type Message = ();
    type Input = PlatformCommand;
    type Output = Result<PlatformResponse, PlatformError>;

    fn create(_scope: &WorkerScope<Self>) -> Self {
        Self {
            engine: Rc::new(Mutex::new(None)),
        }
    }

    fn update(&mut self, _scope: &WorkerScope<Self>, _message: Self::Message) {}

    fn received(&mut self, scope: &WorkerScope<Self>, command: Self::Input, id: HandlerId) {
        let engine = self.engine.clone();
        let scope = scope.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let command = if matches!(command, PlatformCommand::InstallPinnedCatalog) {
                match fetch_pinned_catalog().await {
                    Ok(input) => PlatformCommand::InstallCatalog { input },
                    Err(error) => {
                        scope.respond(id, Err(error));
                        return;
                    }
                }
            } else {
                command
            };
            let mut engine = engine.lock().await;
            if engine.is_none() {
                match IndexedDbStore::open().await {
                    Ok(store) => *engine = Some(BrowserStudioEngine::new(Rc::new(store))),
                    Err(error) => {
                        scope.respond(
                            id,
                            Err(PlatformError::new(
                                PlatformErrorCode::Storage,
                                error.to_string(),
                            )),
                        );
                        return;
                    }
                }
            }
            let now_millis = js_sys::Date::now();
            let now = canonical_now();
            let response = engine
                .as_mut()
                .expect("engine initialized")
                .execute(command, now_millis, now)
                .await;
            scope.respond(id, response);
        });
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogManifest {
    retrieved_at: String,
    cities500_sha256: String,
    admin1_sha256: String,
    admin2_sha256: String,
}

async fn fetch_pinned_catalog() -> Result<CatalogInstallInput, PlatformError> {
    let manifest_bytes = fetch_same_origin("catalog/geonames/manifest.json").await?;
    let manifest: CatalogManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        PlatformError::new(
            PlatformErrorCode::InvalidInput,
            format!("invalid pinned catalog manifest: {error}"),
        )
    })?;
    let manifest_sha256 = sha256(&manifest_bytes);
    let cities500_zip = fetch_same_origin("catalog/geonames/cities500.zip").await?;
    let admin1_codes = fetch_same_origin("catalog/geonames/admin1CodesASCII.txt").await?;
    let admin2_codes = fetch_same_origin("catalog/geonames/admin2Codes.txt").await?;
    for (actual, expected, name) in [
        (
            sha256(&cities500_zip),
            manifest.cities500_sha256.as_str(),
            "cities500.zip",
        ),
        (
            sha256(&admin1_codes),
            manifest.admin1_sha256.as_str(),
            "admin1CodesASCII.txt",
        ),
        (
            sha256(&admin2_codes),
            manifest.admin2_sha256.as_str(),
            "admin2Codes.txt",
        ),
    ] {
        if actual != expected {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidInput,
                format!("pinned GeoNames hash mismatch for {name}"),
            ));
        }
    }
    Ok(CatalogInstallInput {
        cities500_zip,
        admin1_codes,
        admin2_codes,
        retrieved_at: manifest.retrieved_at,
        retrieval: CatalogRetrieval::SameOriginPinned { manifest_sha256 },
    })
}

async fn fetch_same_origin(path: &str) -> Result<Vec<u8>, PlatformError> {
    let global: WorkerGlobalScope = js_sys::global().unchecked_into();
    let value = wasm_bindgen_futures::JsFuture::from(global.fetch_with_str(path))
        .await
        .map_err(|_| {
            PlatformError::new(
                PlatformErrorCode::NotFound,
                format!("same-origin catalog asset {path} is unavailable"),
            )
        })?;
    let response: Response = value.dyn_into().map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::Internal,
            "catalog fetch returned a non-response value",
        )
    })?;
    if !response.ok() {
        return Err(PlatformError::new(
            PlatformErrorCode::NotFound,
            format!(
                "same-origin catalog asset {path} returned HTTP {}",
                response.status()
            ),
        ));
    }
    let buffer = response.array_buffer().map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::Internal,
            "catalog response body is unavailable",
        )
    })?;
    let value = wasm_bindgen_futures::JsFuture::from(buffer)
        .await
        .map_err(|_| {
            PlatformError::new(
                PlatformErrorCode::Internal,
                "catalog response body could not be read",
            )
        })?;
    Ok(js_sys::Uint8Array::new(&value).to_vec())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical_now() -> String {
    let iso = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".into());
    iso.split_once('.')
        .map_or(iso.clone(), |(seconds, _)| format!("{seconds}Z"))
}

pub struct BrowserStudioPlatform {
    bridge: Rc<WorkerBridge<StudioWorker>>,
}

impl BrowserStudioPlatform {
    pub fn spawn() -> Self {
        let mut spawner = StudioWorker::spawner().encoding::<StudioCodec>();
        spawner.with_loader(true).as_module(false);
        Self {
            bridge: Rc::new(spawner.spawn("oracle-studio-worker_loader.js")),
        }
    }
}

impl StudioPlatform for BrowserStudioPlatform {
    fn execute(&self, command: PlatformCommand) -> PlatformFuture {
        let bridge = self.bridge.clone();
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            let sender = Rc::new(RefCell::new(Some(sender)));
            let fork = bridge.fork(Some(move |response| {
                if let Some(sender) = sender.borrow_mut().take() {
                    let _ = sender.send(response);
                }
            }));
            fork.send(command);
            receiver.await.map_err(|_| {
                PlatformError::new(
                    PlatformErrorCode::Internal,
                    "worker response channel closed",
                )
            })?
        })
    }
}
