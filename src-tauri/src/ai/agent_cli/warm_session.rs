//! One desktop-internal warm slot. CLI Polish always uses isolated cold spawns.
use super::*;
use tauri_plugin_store::StoreExt;
use tokio_util::sync::CancellationToken;

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
static SERIAL: Mutex<()> = Mutex::const_new(());
static STATE: LazyLock<parking_lot::Mutex<State>> = LazyLock::new(|| parking_lot::Mutex::new(State::default()));
#[cfg(target_os = "macos")]
const IDLE: Duration = Duration::from_secs(600);

static CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[derive(Default)]
struct State {
    cancellation: CancellationToken,
    generation: u64,
    #[cfg(target_os = "macos")]
    idle: Option<pi_rpc::PiRpc>,
}

pub(crate) fn initialize(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

pub(crate) fn shutdown() {
    CLOSED.store(true, Ordering::SeqCst);
    invalidate_hot_spare();
}

pub(super) fn invalidate() {
    let mut state = STATE.lock();
    state.cancellation.cancel();
    state.cancellation = CancellationToken::new();
    state.generation = state.generation.wrapping_add(1);
    #[cfg(target_os = "macos")]
    { state.idle.take(); }
}

pub(super) fn selected(provider: &str, model: &str) -> bool {
    if CLOSED.load(Ordering::SeqCst) { return false; }
    let Some(app) = APP.get() else { return false; };
    let Ok(store) = app.store("settings") else { return false; };
    store.get("ai_enabled").and_then(|v| v.as_bool()) == Some(true)
        && store.get("ai_provider").and_then(|v| v.as_str().map(str::to_owned)).as_deref() == Some(provider)
        && store.get("ai_model").and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default() == model
}

pub(super) async fn run(
    provider: &str, model: &str, binary: &Path, argv: &[String], input: &str, help: &[u8], budget: Duration,
) -> Option<Result<String, MappedAiProviderError>> {
    if !selected(provider, model) { return None; }
    let started = Instant::now();
    let _serial = tokio::time::timeout(budget, SERIAL.lock()).await.ok()?;
    let budget = budget.saturating_sub(started.elapsed());
    if budget.is_zero() { return None; }
    if !selected(provider, model) { return None; }
    let (generation, cancellation) = {
        let mut state = STATE.lock();
        state.generation = state.generation.wrapping_add(1);
        (state.generation, state.cancellation.clone())
    };
    let attempt = async {
        if provider == PROVIDER_CLAUDE_CODE {
            return CLAUDE_SPARE.take_and_run_with_budget(binary, argv, input, budget).await;
        }
        #[cfg(target_os = "macos")]
        if provider == PROVIDER_PI {
            let idle = STATE.lock().idle.take();
            let mut rpc = match idle {
                Some(rpc) if rpc.binary == binary && rpc.argv == argv => rpc,
                old => {
                    drop(old);
                    match pi_rpc::PiRpc::spawn(binary, argv, help).await {
                        Ok(rpc) => rpc,
                        Err(error) => return Some(Err(error)),
                    }
                }
            };
            let result = rpc.run(input).await;
            if result.is_ok() {
                let epoch = {
                    let mut state = STATE.lock();
                    if state.generation != generation || cancellation.is_cancelled() { return None; }
                    state.generation = state.generation.wrapping_add(1);
                    state.idle = Some(rpc);
                    state.generation
                };
                tokio::spawn(async move {
                    tokio::time::sleep(IDLE).await;
                    let mut state = STATE.lock();
                    if state.generation == epoch {
                        state.idle.take();
                        state.generation = state.generation.wrapping_add(1);
                    }
                });
            }
            return Some(result);
        }
        let _ = (generation, help);
        None
    };
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => None,
        result = tokio::time::timeout(budget.min(Duration::from_secs(10)), attempt) => result.ok().flatten(),
    }
}


pub(super) async fn prepare(request: &AiPolishRequest, expected_epoch: u64) {
    #[cfg(target_os = "macos")]
    {
        if !selected(&request.provider_id, &request.model_id) { return; }
        let Ok(_serial) = tokio::time::timeout(Duration::from_secs(10), SERIAL.lock()).await else { return; };
        let cancellation = STATE.lock().cancellation.clone();
        let task = async {
            let binary = resolve_binary(PI_SPEC.binary).await?;
            let (capabilities, help, _) = discover_capabilities_for_polish(&binary, &PI_SPEC, false).await;
            if !required_capabilities_present(&PI_SPEC, &help) { return None; }
            let argv = cold_argv_for_model_with_options(
                &PI_SPEC, &request.prompt, &request.model_id, capabilities,
                request.reasoning_level.as_deref(), request.fast_mode,
            );
            {
                let mut state = STATE.lock();
                if state.idle.as_ref().is_some_and(|rpc| rpc.binary == binary && rpc.argv == argv) { return Some(()); }
                state.idle.take();
            }
            let rpc = pi_rpc::PiRpc::spawn(&binary, &argv, &help).await.ok()?;
            let generation = {
                let mut state = STATE.lock();
                if cancellation.is_cancelled() || hot_spare_generation() != expected_epoch { return None; }
                state.generation = state.generation.wrapping_add(1);
                state.idle = Some(rpc);
                state.generation
            };
            tokio::spawn(async move {
                tokio::time::sleep(IDLE).await;
                let mut state = STATE.lock();
                if state.generation == generation { state.idle.take(); }
            });
            Some(())
        };
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {},
            _ = tokio::time::timeout(Duration::from_secs(10), task) => {},
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (request, expected_epoch);
}

pub(super) async fn admission() -> Option<tokio::sync::MutexGuard<'static, ()>> {
    tokio::time::timeout(Duration::from_secs(10), SERIAL.lock()).await.ok()
}

pub(super) fn replenish(provider: &str, model: &str, binary: &Path, argv: &[String], help: &[u8]) {
    if !cfg!(target_os = "macos") || provider != PROVIDER_CLAUDE_CODE || !selected(provider, model)
        || !help_advertises_flag(help, "--input-format") || !help_advertises_flag(help, "--output-format") { return; }
    let model = model.to_owned();
    let binary = binary.to_owned();
    let argv = argv.to_vec();
    let epoch = hot_spare_generation();
    let spare_generation = CLAUDE_SPARE.generation();
    let cancellation = STATE.lock().cancellation.clone();
    tokio::spawn(async move {
        let Some(_serial) = admission().await else { return; };
        if epoch != hot_spare_generation() || !selected(PROVIDER_CLAUDE_CODE, &model) { return; }
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {},
            _ = CLAUDE_SPARE.warm_if_current(&binary, &argv, spare_generation) => {},
        }
    });
}
