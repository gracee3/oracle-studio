#[cfg(target_arch = "wasm32")]
use gloo_worker::Registrable;

#[cfg(target_arch = "wasm32")]
fn main() {
    oracle_studio_worker::StudioWorker::registrar()
        .encoding::<oracle_studio_worker::StudioCodec>()
        .register();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
