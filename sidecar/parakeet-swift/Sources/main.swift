import Foundation
import Darwin
@preconcurrency import AVFoundation
import FluidAudio

// Keep a duplicate of the real protocol stdout so progress events still reach
// Tauri while native library calls temporarily redirect STDOUT_FILENO.

let protocolStdoutFileDescriptor = dup(STDOUT_FILENO)
let protocolWriteQueue = DispatchQueue(label: "com.voicetypr.parakeet.protocol-writes")

func writeProtocolLine(_ line: String) {
    protocolWriteQueue.sync {
        let outputFileDescriptor = protocolStdoutFileDescriptor >= 0 ? protocolStdoutFileDescriptor : STDOUT_FILENO
        var data = Data(line.utf8)
        data.append(0x0A)

        data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress else {
                return
            }

            var bytesWritten = 0
            while bytesWritten < buffer.count {
                let result = Darwin.write(
                    outputFileDescriptor,
                    baseAddress.advanced(by: bytesWritten),
                    buffer.count - bytesWritten
                )
                if result <= 0 {
                    return
                }
                bytesWritten += result
            }
        }
    }
}

// Helper function to log to stderr (so it doesn't interfere with JSON on stdout)
func log(_ message: String) {
    fputs("\(message)\n", stderr)
    fflush(stderr)
}

/// Blocking `readLine()` on a dedicated thread so the MainActor event loop can yield to
/// drain tasks. The stream is `.bufferingOldest(64)`: at most 64 raw command lines are
/// queued ahead of the consumer. The default unbounded AsyncStream would let a stalled
/// consumer (e.g. a long inference) accumulate unbounded command memory; the cap bounds it.
///
/// Because `.bufferingOldest` drops the NEW element when full, the producer applies
/// backpressure: on `.dropped` it sleeps briefly and re-yields the SAME line until the
/// consumer makes room (`.enqueued`). This preserves every line in FIFO order — a
/// `finalize_stream`/`cancel_stream` line can NEVER be dropped, since those terminal
/// commands must reach the consumer to end the session; dropping them would hang the
/// host. `readLine()` itself is naturally blocking, so the backpressure sleep only adds
/// latency (no busy spin under throughput) and is bounded by the consumer. On
/// `.terminated` the producer exits so a torn-down consumer releases the thread.
enum ProtocolStdin {
    static let maxCommandBacklog = 64

    static func lineStream() -> AsyncStream<String> {
        AsyncStream<String>(bufferingPolicy: .bufferingOldest(maxCommandBacklog)) { continuation in
            let thread = Thread {
                while let line = readLine() {
                    while true {
                        switch continuation.yield(line) {
                        case .enqueued:
                            break
                        case .dropped:
                            Thread.sleep(forTimeInterval: 0.002)
                            continue
                        case .terminated:
                            return
                        @unknown default:
                            Thread.sleep(forTimeInterval: 0.002)
                            continue
                        }
                        break
                    }
                }
                continuation.finish()
            }
            thread.name = "parakeet-protocol-stdin"
            thread.start()
        }
    }
}


// Get system architecture info
func getArchitectureInfo() -> String {
    #if arch(arm64)
    return "arm64 (Apple Silicon)"
    #elseif arch(x86_64)
    return "x86_64 (Intel)"
    #else
    return "unknown"
    #endif
}

// FluidAudio/CoreML can write diagnostics directly to stdout from native code.
// Stdout is our line-delimited JSON protocol, so run library calls with stdout
// temporarily redirected to stderr and restore it before sending responses.
@MainActor
func withLibraryStdoutRedirected<T>(_ operation: @MainActor () async throws -> T) async throws -> T {
    fflush(stdout)
    let savedStdout = dup(STDOUT_FILENO)
    guard savedStdout >= 0 else {
        return try await operation()
    }

    if dup2(STDERR_FILENO, STDOUT_FILENO) < 0 {
        close(savedStdout)
        return try await operation()
    }

    defer {
        fflush(stdout)
        dup2(savedStdout, STDOUT_FILENO)
        close(savedStdout)
    }

    return try await operation()
}

// Log system information for debugging
func logSystemInfo() {
    log("🦜 Parakeet sidecar started")
    log("   Architecture: \(getArchitectureInfo())")
    log("   macOS: \(ProcessInfo.processInfo.operatingSystemVersionString)")
    log("   PID: \(ProcessInfo.processInfo.processIdentifier)")
}



struct OkResponse: Encodable {
    let type: String = "ok"
    let command: String
}

// JSON message structures for communication with Tauri
struct TranscriptionResponse: Encodable {
    let type: String = "transcription"
    let text: String
    let segments: [Segment]
    let language: String?
    let duration: Float?

    init(text: String, segments: [Segment] = [], language: String? = nil, duration: Float? = nil) {
        self.text = text
        self.segments = segments
        self.language = language
        self.duration = duration
    }
}

struct DiarizationResponse: Encodable {
    let type: String = "diarization"
    let segments: [SpeakerSegment]
}

struct SpeakerSegment: Encodable {
    let speakerId: String
    let start: Float
    let end: Float
}

struct Segment: Encodable {
    let text: String
}

struct StatusResponse: Encodable {
    let type: String = "status"
    let loadedModel: String?
    let modelVersion: String?
    let modelPath: String? = nil
    let precision: String? = nil
    let attention: String? = nil
}

struct ProgressResponse: Encodable {
    let type: String = "progress"
    let progress: Double
    let phase: String
}

struct StreamStartedResponse: Encodable {
    let type: String = "stream_started"
}

struct StreamPartialResponse: Encodable {
    let type: String = "stream_partial"
    let text: String
    let isConfirmed: Bool
    let confidence: Float

    enum CodingKeys: String, CodingKey {
        case type
        case text
        case isConfirmed = "is_confirmed"
        case confidence
    }
}

struct StreamFinalResponse: Encodable {
    let type: String = "stream_final"
    let text: String
}

struct StreamCancelledResponse: Encodable {
    let type: String = "stream_cancelled"
}

struct EouModelStatusResponse: Encodable {
    let type: String = "eou_model_status"
    let chunkMs: Int
    let downloaded: Bool
    let path: String?
}

struct ErrorResponse: Encodable {
    let type: String = "error"
    let code: String
    let message: String
    let details: [String: String]? = nil  // Optional details field to match Rust
}

struct WarmedResponse: Encodable {
    let type: String = "warmed"
    let warmed: Bool
    let ms: Int
    let error: String?
}

enum SupportedModelVersion: String, CaseIterable {
    case v2
    case v3

    var asrVersion: AsrModelVersion {
        switch self {
        case .v2: return .v2
        case .v3: return .v3
        }
    }

    var modelIdentifier: String {
        switch self {
        case .v2: return "parakeet-tdt-0.6b-v2"
        case .v3: return "parakeet-tdt-0.6b-v3"
        }
    }

    var repoFolderName: String {
        modelIdentifier
    }
}
enum SupportedModel: Hashable {
    case tdt(SupportedModelVersion)
    case unified640
    case nemotronMultilingual1120

    var modelIdentifier: String {
        switch self {
        case .tdt(let version): return version.modelIdentifier
        case .unified640: return "parakeet-unified-640ms"
        case .nemotronMultilingual1120: return "nemotron-multilingual-1120ms"
        }
    }

    var wireVersion: String {
        switch self {
        case .tdt(let version): return version.rawValue
        case .unified640: return "unified_640"
        case .nemotronMultilingual1120: return "nemotron_multilingual_1120"
        }
    }

    func supportsStreamEngine(_ engine: String) -> Bool {
        switch self {
        case .tdt:
            return engine == "sliding_window" || engine == "eou" || engine == "decode_ahead"
        case .unified640:
            return engine == "unified_english"
        case .nemotronMultilingual1120:
            return engine == "nemotron_multilingual"
        }
    }
}


// Global ASR manager state
@MainActor var asrManager: AsrManager?
@MainActor var loadedAsrModels: AsrModels?
@MainActor var isModelLoaded = false
@MainActor var loadedModelVersion: SupportedModelVersion?
@MainActor var downloadedVersions = Set<SupportedModelVersion>()
@MainActor var loadedModel: SupportedModel?
@MainActor var unifiedManager: StreamingUnifiedAsrManager?
@MainActor var nemotronMultilingualManager: StreamingNemotronMultilingualAsrManager?
@MainActor var cachedEouManagers: [Int: StreamingEouAsrManager] = [:]


@MainActor
final class ActiveStreamSession {
    enum Engine {
        case slidingWindow(SlidingWindowAsrManager)
        case eou(StreamingEouAsrManager)
        case decodeAhead(DecodeAheadAsrSession)
        case unified(StreamingUnifiedAsrManager)
        case nemotronMultilingual(StreamingNemotronMultilingualAsrManager)
    }

    let engine: Engine
    let sampleRate: Double
    let channels: Int
    let encoder: JSONEncoder
    var forwarder: Task<Void, Never>?
    var committedPrefix = ""
    var latestPartial = ""
    /// Bounded ingress for decode_ahead: FIFO PCM buffers, one drain task, overflow dropped.
    private static let decodeAheadMaxPendingBuffers = 32
    var decodeAheadPendingBuffers: [AVAudioPCMBuffer] = []
    var decodeAheadDrainTask: Task<Void, Never>?
    var decodeAheadNoMoreInput = false

    init(engine: Engine, sampleRate: Double, channels: Int, encoder: JSONEncoder) {
        self.engine = engine
        self.sampleRate = sampleRate
        self.channels = channels
        self.encoder = encoder
    }

    func enqueueDecodeAheadChunk(_ buffer: AVAudioPCMBuffer, decodeSession: DecodeAheadAsrSession) {
        guard !decodeAheadNoMoreInput else {
            log("⚠️ decode_ahead: ignoring audio_chunk after no-more-input")
            return
        }
        if decodeAheadPendingBuffers.count >= Self.decodeAheadMaxPendingBuffers {
            decodeAheadPendingBuffers.removeFirst()
            log("⚠️ decode_ahead: dropped oldest pending audio chunk (bounded queue)")
        }
        decodeAheadPendingBuffers.append(buffer)
        ensureDecodeAheadDrain(decodeSession: decodeSession)
    }

    private func ensureDecodeAheadDrain(decodeSession: DecodeAheadAsrSession) {
        guard !decodeAheadNoMoreInput, !decodeAheadPendingBuffers.isEmpty else { return }
        if decodeAheadDrainTask != nil {
            return
        }
        decodeAheadDrainTask = Task { @MainActor in
            defer {
                self.decodeAheadDrainTask = nil
                if !self.decodeAheadNoMoreInput, !self.decodeAheadPendingBuffers.isEmpty {
                    self.ensureDecodeAheadDrain(decodeSession: decodeSession)
                }
            }
            await self.runDecodeAheadDrain(decodeSession: decodeSession)
        }
    }

    private func runDecodeAheadDrain(decodeSession: DecodeAheadAsrSession) async {
        while !Task.isCancelled, !decodeAheadNoMoreInput {
            guard !decodeAheadPendingBuffers.isEmpty else {
                return
            }
            let chunk = decodeAheadPendingBuffers.removeFirst()
            await decodeSession.ingestSamples(chunk)
            if Task.isCancelled { return }
            await decodeSession.runLiveDecodeIfNeeded()
            if Task.isCancelled { return }
        }
    }

    func finalizeDecodeAhead(_ decodeSession: DecodeAheadAsrSession) async -> String {
        decodeAheadNoMoreInput = true
        if let drain = decodeAheadDrainTask {
            await drain.value
        }
        while !decodeAheadPendingBuffers.isEmpty {
            let chunk = decodeAheadPendingBuffers.removeFirst()
            await decodeSession.ingestSamples(chunk)
            await decodeSession.runLiveDecodeIfNeeded()
        }
        decodeAheadDrainTask = nil
        return await decodeSession.finalize()
    }

    /// Prompt cancel: do not await drain; clear queue and invalidate in-flight decode immediately.
    func cancelDecodeAheadImmediately(_ decodeSession: DecodeAheadAsrSession) async {
        decodeAheadDrainTask?.cancel()
        decodeAheadPendingBuffers.removeAll()
        decodeAheadNoMoreInput = true
        decodeAheadDrainTask = nil
        await decodeSession.cancel()
    }
}

/// Decode-ahead live-preview ASR session (plan 051, Phase 1): a faithful Swift port of
/// `whisper/decode_ahead.rs::DecodeAheadBuffer` fused with its driver. A growing
/// `samples` buffer plus a `head` index; each decode re-runs `AsrManager.transcribe` on
/// the whole un-committed window (fresh decoder state — no KV reuse), then commits only
/// by token timestamp so boundary-cut words stay revisable. This is the decode-ahead
/// fix for FluidAudio's SlidingWindow engine permanently baking each chunk's tokens at
/// decode time.
///
/// Ingress is bounded on `ActiveStreamSession` (FIFO, one drain task). `audio_chunk` enqueues
/// and returns; cancel clears the queue and bumps `emissionGeneration` without waiting on drain.
/// Finalize sets no-more-input, awaits the drain task, then eos-finalizes. Emissions are
/// generation-gated. Routed via `writeProtocolLine` (dup'd fd).
actor DecodeAheadAsrSession {
    // MARK: - Configuration (16 kHz mono f32; mirrors decode_ahead.rs `Config`)

    /// Sample rate assumed throughout (16 kHz mono f32).
    private static let sampleRate = 16_000
    /// Minimum un-decoded samples before a decode is worth running (~1 s). Below this,
    /// `shouldDecode` short-circuits to false (unless `eos`).
    private static let minSamples = 16_000
    /// Re-decode once the window has grown this many samples since the last attempt (~1 s).
    private static let incrSamples = 16_000
    /// Hard cap: force a decode (committing ALL tokens) once the window reaches this
    /// (14 s — the model input is fixed 15 s; leave 1 s slack so each pass is a single
    /// coherent decode rather than an internally-chunked one).
    private static let maxWindowSamples = 224_000
    /// Tokens whose end falls within this many seconds of the window tail stay tentative
    /// (revisable). On eos/finalize, ALL tokens are committed regardless of margin.
    private static let tailMarginSeconds = 1.5
    /// Maximum backoff shift: `incrSamples * 2^N`, capped at `2^4 = 16×`.
    private static let maxBackoffShift = 4
    /// Bumped on `cancel()`; decode passes capture the value at start and suppress emission
    /// when it no longer matches (stale partial after cancel).
    private var emissionGeneration: UInt64 = 0

    private struct DecodeAheadDecodeResult {
        enum Outcome {
            case failure
            case success(timings: [TokenTiming], text: String, modelReturnedTimings: Bool)
        }
        let outcome: Outcome
    }

    // MARK: - State (faithful port of DecodeAheadBuffer field structure)

    private var samples: [Float] = []
    /// Start of the un-committed window; rebased to 0 by `maybeCompact` after compaction.
    private var head = 0
    private var committed = ""
    /// Window-length threshold at/after which the next decode is allowed. Compared
    /// against the un-committed window LENGTH (a length, not an absolute index).
    private var nextInferAtLen: Int
    /// Consecutive decodes that committed nothing; drives exponential backoff.
    private var noProgressRuns = 0
    /// Single shared resampler (stateless): input is device-rate → 16 k mono f32.
    private let converter = AudioConverter()
    /// Weak ref to the app-wide loaded AsrManager (kept alive by the global
    /// `asrManager`); unload is blocked while a stream is active, so this is never
    /// released mid-stream in practice.
    private weak var manager: AsrManager?
    private let decoderLayers: Int
    private let encoder: JSONEncoder

    init(manager: AsrManager, decoderLayers: Int, encoder: JSONEncoder) {
        self.manager = manager
        self.decoderLayers = decoderLayers
        self.encoder = encoder
        self.nextInferAtLen = Self.minSamples
    }

    // MARK: - Driver (fused with the pure buffer)

    /// Append resampled samples only (drain task); decode is separate so ingress stays bounded.
    func ingestSamples(_ buffer: AVAudioPCMBuffer) async {
        guard let resampled = try? converter.resampleBuffer(buffer), !resampled.isEmpty else {
            log("⚠️ decode_ahead: failed to resample audio chunk; skipping")
            return
        }
        samples.append(contentsOf: resampled)
    }

    /// One live preview decode when `shouldDecode` permits (called from the drain task).
    func runLiveDecodeIfNeeded() async {
        guard !Task.isCancelled else { return }
        guard shouldDecode(eos: false), let manager = manager else {
            return
        }
        let passGeneration = emissionGeneration
        let window = currentWindow()
        let decodeResult = await Self.decode(manager: manager, window: window, decoderLayers: decoderLayers)
        if Task.isCancelled || emissionGeneration != passGeneration {
            return
        }
        applyDecodePass(
            decodeResult: decodeResult,
            eos: false,
            decodedWindowLen: window.count,
            passGeneration: passGeneration,
            emitPartials: true
        )
    }

    func appendChunk(_ buffer: AVAudioPCMBuffer) async {
        await ingestSamples(buffer)
        await runLiveDecodeIfNeeded()
    }

    /// Drain ALL remaining audio with eos passes (commit everything), returning the
    /// full committed transcript. A LOOP, not a single pass (Codex 051 finding): one
    /// capped decode covers at most `maxWindowSamples`, so a recording whose tail
    /// extends past the cap — e.g. after long silence pinned the window — needs
    /// repeated passes.
    ///
    /// Consumption is tracked as a LENGTH, never as pre-pass absolute indices
    /// (Codex 051 round-2 finding): `ingest` may compact-and-rebase the buffer
    /// internally, so `preHead + window.count` arithmetic against the rebased array
    /// would overshoot and silently discard un-decoded audio. The remaining length is
    /// well-defined in every coordinate system: after a pass that decoded
    /// `window.count` samples and committed every token in them, the remaining length
    /// must be exactly `availableBefore - window.count` (the un-tokenized remainder of
    /// the window is silence). A decode failure aborts the drain — better to return
    /// the committed-so-far text than to consume audio that was never decoded. No
    /// partial emission — the caller sends `stream_final`, which replaces any stale
    /// tentative in the pill.
    func finalize() async -> String {
        finalizeDrain: while let manager = manager {
            let availableBefore = samples.count - head
            if availableBefore <= 0 {
                break finalizeDrain
            }
            let window = currentWindow()
            let decodeResult = await Self.decode(
                manager: manager, window: window, decoderLayers: decoderLayers)
            if Task.isCancelled { break finalizeDrain }
            switch decodeResult.outcome {
            case .failure:
                break finalizeDrain
            case .success(let timings, let text, let modelReturnedTimings):
                if timings.isEmpty, !text.isEmpty, !modelReturnedTimings {
                    committed += detokenizeNormalizedText(text, leadingContent: !committed.isEmpty)
                    let targetRemaining = availableBefore - window.count
                    let currentRemaining = samples.count - head
                    if currentRemaining > targetRemaining {
                        head += currentRemaining - targetRemaining
                    }
                    maybeCompact()
                    if targetRemaining <= 0 {
                        break finalizeDrain
                    }
                    continue finalizeDrain
                }
                _ = ingest(
                    timings: timings,
                    eos: true,
                    decodedWindowLen: window.count,
                    fallbackText: text,
                    modelReturnedTimings: modelReturnedTimings
                )
                let targetRemaining = availableBefore - window.count
                let currentRemaining = samples.count - head
                if currentRemaining > targetRemaining {
                    head += currentRemaining - targetRemaining
                }
                maybeCompact()
                if targetRemaining <= 0 {
                    break finalizeDrain
                }
            }
        }
        return committed
    }

    /// Drop all state; bump generation so in-flight decode passes suppress emission.
    func cancel() async {
        emissionGeneration &+= 1
        samples.removeAll()
        head = 0
        committed = ""
        noProgressRuns = 0
        nextInferAtLen = Self.minSamples
    }

    private func applyDecodePass(
        decodeResult: DecodeAheadDecodeResult,
        eos: Bool,
        decodedWindowLen: Int,
        passGeneration: UInt64,
        emitPartials: Bool
    ) {
        switch decodeResult.outcome {
        case .failure:
            return
        case .success(let timings, let text, let modelReturnedTimings):
            let (grew, tentative) = ingest(
                timings: timings,
                eos: eos,
                decodedWindowLen: decodedWindowLen,
                fallbackText: text,
                modelReturnedTimings: modelReturnedTimings
            )
            guard emitPartials, emissionGeneration == passGeneration else { return }
            emit(grewCommitted: grew, tentative: tentative)
        }
    }

    // MARK: - Pure buffer logic (port of DecodeAheadBuffer)

    /// Decide whether to decode now (port of `DecodeAheadBuffer::should_decode`).
    /// - Skip while `!eos && window < minSamples` (not enough audio yet).
    /// - Force when `eos` or `window >= maxWindowSamples`.
    /// - Otherwise decode once the window reaches `nextInferAtLen`.
    private func shouldDecode(eos: Bool) -> Bool {
        let len = samples.count - head
        if !eos && len < Self.minSamples {
            return false
        }
        if eos || len >= Self.maxWindowSamples {
            return true
        }
        return len >= nextInferAtLen
    }

    /// The un-decoded tail `samples[head...]`, capped at `maxWindowSamples`.
    private func currentWindow() -> [Float] {
        let available = samples.count - head
        if available > Self.maxWindowSamples {
            log("⚠️ decode_ahead: window \(available) exceeds max \(Self.maxWindowSamples); capping (compaction should have bounded this)")
        }
        let take = min(available, Self.maxWindowSamples)
        return Array(samples[head..<(head + take)])
    }

    /// Absorb a fresh decode's token timings and produce the preview partial.
    ///
    /// Commit rule (token-timing adaptation of the Rust segment rule, hardened by the
    /// 2026-07-10 bench): a naive `endTime <= windowSeconds - tailMargin` cut commits
    /// MID-WORD ("transcri Egyptian", "Whiskey change" for "risky chain") because head
    /// then advances into the middle of a word and the next decode starts on half a
    /// word. Whisper never hit this because its segments end at natural pauses — so we
    /// recreate that: the cut may only fall where (a) the NEXT token starts a new word
    /// (leading SentencePiece `▁` or FluidAudio 0.15.5 normalized leading space) AND (b)
    /// there is an inter-token silence gap of at least `pauseGapSeconds`. Head then
    /// advances to MID-GAP, so the next window starts in silence, never mid-phoneme.
    /// When the window hits `maxWindowSamples` the gap requirement is dropped (word
    /// boundary alone) to guarantee forward progress; on eos ALL tokens are committed
    /// (nothing follows). `tentative` is the detok of everything after the cut and stays
    /// fully revisable.
    private static let pauseGapSeconds = 0.15

    private func ingest(
        timings: [TokenTiming],
        eos: Bool,
        decodedWindowLen: Int,
        fallbackText: String,
        modelReturnedTimings: Bool
    ) -> (grew: Bool, tentative: String) {
        // `maxWindowSamples` — the un-committed tail can exceed it while decodes lag.
        // The commit threshold must therefore come from the decoded length, not the
        // full remaining length (a too-high threshold would silently stop commits).
        let windowSeconds = Double(decodedWindowLen) / Double(Self.sampleRate)
        let atMaxWindow = samples.count - head >= Self.maxWindowSamples
        let threshold = windowSeconds - Self.tailMarginSeconds

        // Find the cut: the last margin-eligible index where the boundary is safe.
        // eos commits everything; atMaxWindow accepts a bare word boundary; otherwise
        if eos, timings.isEmpty, !fallbackText.isEmpty {
            committed += detokenizeNormalizedText(fallbackText, leadingContent: !committed.isEmpty)
            return (true, "")
        }
        // require word boundary + pause gap.
        var cutIndex = -1 // commit timings[0...cutIndex]
        var advanceSeconds = 0.0
        if eos {
            cutIndex = timings.count - 1
            advanceSeconds = timings.last.map(\.endTime) ?? 0.0
        } else {
            for i in timings.indices {
                guard timings[i].endTime <= threshold else { break }
                guard i + 1 < timings.count else {
                    // Margin-eligible with NO following token: the tail margin is
                    // trailing silence, so cutting at endTime is safe.
                    cutIndex = i
                    advanceSeconds = timings[i].endTime
                    continue
                }
                let next = timings[i + 1]
                guard Self.tokenStartsNewWord(next.token) else { continue }
                let gap = next.startTime - timings[i].endTime
                if gap >= Self.pauseGapSeconds {
                    cutIndex = i
                    // Advance to mid-gap: the next window starts in silence.
                    advanceSeconds = timings[i].endTime + gap / 2.0
                } else if atMaxWindow {
                    // Forced progress at the window cap: word boundary alone.
                    cutIndex = i
                    advanceSeconds = timings[i].endTime
                }
            }
        }

        let committedTimings = cutIndex >= 0 ? Array(timings[...cutIndex]) : []
        let tentativeTimings = Array(timings[(cutIndex + 1)...])

        let grew = !committedTimings.isEmpty
        if grew {
            appendCommitted(committedTimings.map(\.token))
            let advanceSamples = Int((advanceSeconds * Double(Self.sampleRate)).rounded())
            let maxAdvance = samples.count - head // never advance past the end
            head += min(max(advanceSamples, 0), maxAdvance)
        } else if !eos && atMaxWindow && timings.isEmpty && fallbackText.isEmpty {
            let retain = Int(Self.tailMarginSeconds * Double(Self.sampleRate))
            let consume = max(min(decodedWindowLen, samples.count - head) - retain, 0)
            head += consume
        } else if !eos && atMaxWindow && timings.isEmpty && !fallbackText.isEmpty && !modelReturnedTimings {
            committed += detokenizeNormalizedText(fallbackText, leadingContent: !committed.isEmpty)
            let consume = min(decodedWindowLen, samples.count - head)
            head += consume
            noProgressRuns = 0
            nextInferAtLen = (samples.count - head) + Self.incrSamples
            maybeCompact()
            return (true, "")
        } else if !eos && timings.isEmpty && !fallbackText.isEmpty && !modelReturnedTimings {
            noProgressRuns = 0
            let step = Self.incrSamples
            nextInferAtLen = (samples.count - head) + step
            maybeCompact()
            let tentative = detokenizeNormalizedText(fallbackText, leadingContent: !committed.isEmpty)
            return (false, tentative)
        }

        // Backoff + next-infer schedule, relative to the post-advance window length.
        // For PREVIEW, a growing tentative is progress too (2026-07-10 bench: keying
        // backoff on committed-growth alone starved the pill to 2-3 updates per clip,
        // because pause-gated commits are rare in continuous speech). Back off only on
        // true silence: a decode that produced NO tokens at all.
        let madeProgress = grew || !timings.isEmpty
        noProgressRuns = madeProgress ? 0 : noProgressRuns + 1
        let shift = min(noProgressRuns, Self.maxBackoffShift)
        let step = madeProgress ? Self.incrSamples : Self.incrSamples << shift
        nextInferAtLen = (samples.count - head) + step

        maybeCompact()

        let tentative = detokenize(tentativeTimings.map(\.token), leadingContent: !committed.isEmpty)
        return (grew, tentative)
    }

    /// Reclaim consumed samples once ≥1 s has been committed-and-skipped OR the head has
    /// passed the halfway mark. Bounds memory for long recordings. Drains
    /// `samples[..<head]` and resets `head` to 0; does NOT change the window length, so
    /// `nextInferAtLen` needs no adjustment.
    private func maybeCompact() {
        if head >= Self.sampleRate || head > samples.count / 2 {
            samples.removeFirst(head)
            head = 0
            // REGRESSION GUARD (plan 051): do NOT adjust `nextInferAtLen` here. It is
            // relative to the un-committed window LENGTH, which compaction leaves
            // unchanged; rebasing it after draining would collapse the grow-gap and
            // trigger premature re-decode thrash right after every compaction. Mirrors
            // the GLM-caught fix in whisper/decode_ahead.rs::maybe_compact.
        }
    }

    /// FluidAudio 0.15.5 may normalize word starts as leading ASCII space or SentencePiece `▁`.
    private static func tokenStartsNewWord(_ token: String) -> Bool {
        token.hasPrefix("\u{2581}") || token.hasPrefix(" ")
    }

    private func appendCommitted(_ pieces: [String]) {
        committed += detokenize(pieces, leadingContent: !committed.isEmpty)
    }

    /// Detokenize SentencePiece pieces or a normalized fallback string: word starts are
    /// `▁` or a leading space; subword pieces glue without a separator.
    private func detokenize(_ pieces: [String], leadingContent: Bool) -> String {
        var result = ""
        var hasContent = leadingContent
        for piece in pieces {
            if piece.isEmpty || piece == "<blank>" || piece == "<pad>" {
                continue
            }
            if Self.tokenStartsNewWord(piece) {
                let rest: String
                if piece.hasPrefix("\u{2581}") {
                    rest = String(piece.dropFirst())
                } else {
                    rest = String(piece.dropFirst()).trimmingCharacters(in: .whitespaces)
                }
                if rest.isEmpty {
                    if hasContent { result += " " }
                } else {
                    if hasContent { result += " " }
                    result += rest
                    hasContent = true
                }
            } else {
                result += piece
                hasContent = true
            }
        }
        return result
    }

    private func detokenizeNormalizedText(_ text: String, leadingContent: Bool) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        if leadingContent, !trimmed.hasPrefix(" ") {
            return " " + trimmed
        }
        return trimmed
    }

    // MARK: - Emission

    /// Emit the protocol partials after a decode pass. `is_confirmed: true` carries the
    /// FULL cumulative committed string (byte-prefix monotonic); `is_confirmed: false`
    /// carries ONLY the tentative tail (replaced wholesale each time, even when empty,
    /// to clear the pill's stale tail). Routed via `writeProtocolLine` (dup'd fd), so it
    /// is immune to the native-stdout redirect around the decode itself.
    private func emit(grewCommitted: Bool, tentative: String) {
        if grewCommitted {
            ParakeetSidecar.sendResponse(
                StreamPartialResponse(text: committed, isConfirmed: true, confidence: 1.0),
                encoder: encoder
            )
        }
        ParakeetSidecar.sendResponse(
            StreamPartialResponse(text: tentative, isConfirmed: false, confidence: 0.0),
            encoder: encoder
        )
    }

    // MARK: - Decode (isolated to the main actor for the native-stdout redirect)

    /// Run one fresh coherent decode of `window`, returning text plus timings. `failure`
    /// is distinct from successful empty timings (silence). Missing `tokenTimings` with
    /// nonempty `text` is reported via `modelReturnedTimings: false` for preview fallback.
    @MainActor
    private static func decode(
        manager: AsrManager,
        window: [Float],
        decoderLayers: Int
    ) async -> DecodeAheadDecodeResult {
        var state = TdtDecoderState.make(decoderLayers: decoderLayers)
        do {
            let result = try await withLibraryStdoutRedirected {
                try await manager.transcribe(window, decoderState: &state)
            }
            let text = result.text
            if let timings = result.tokenTimings {
                return DecodeAheadDecodeResult(
                    outcome: .success(timings: timings, text: text, modelReturnedTimings: true)
                )
            }
            return DecodeAheadDecodeResult(
                outcome: .success(timings: [], text: text, modelReturnedTimings: false)
            )
        } catch {
            log("⚠️ decode_ahead: transcribe failed: \(error.localizedDescription)")
            return DecodeAheadDecodeResult(outcome: .failure)
        }
    }
}

/// Deterministic token-normalization checks for Main (`swift run ParakeetSidecar --decode-ahead-token-harness`).
enum DecodeAheadTokenNormalizationHarness {
    private static func detokenizePieces(_ pieces: [String], leadingContent: Bool) -> String {
        var result = ""
        var hasContent = leadingContent
        for piece in pieces {
            if piece.isEmpty || piece == "<blank>" || piece == "<pad>" {
                continue
            }
            let wordStart = piece.hasPrefix("\u{2581}") || piece.hasPrefix(" ")
            if wordStart {
                let rest: String
                if piece.hasPrefix("\u{2581}") {
                    rest = String(piece.dropFirst())
                } else {
                    rest = String(piece.dropFirst()).trimmingCharacters(in: .whitespaces)
                }
                if rest.isEmpty {
                    if hasContent { result += " " }
                } else {
                    if hasContent { result += " " }
                    result += rest
                    hasContent = true
                }
            } else {
                result += piece
                hasContent = true
            }
        }
        return result
    }

    static func run() {
        var failures = 0
        func check(_ name: String, _ ok: Bool) {
            if ok {
                fputs("PASS \(name)\n", stderr)
            } else {
                fputs("FAIL \(name)\n", stderr)
                failures += 1
            }
        }

        check(
            "sentencepiece_word_boundary",
            detokenizePieces(["\u{2581}Hello", "\u{2581}world"], leadingContent: false) == "Hello world"
        )
        check(
            "normalized_leading_space_word_boundary",
            detokenizePieces([" Hello", " world"], leadingContent: false) == "Hello world"
        )
        check(
            "subword_glue",
            detokenizePieces(["trans", "cript"], leadingContent: false) == "transcript"
        )
        check(
            "word_start_detection_space",
            " world".hasPrefix(" ") || " world".hasPrefix("\u{2581}")
        )
        check(
            "word_start_detection_sentencepiece",
            "\u{2581}word".hasPrefix("\u{2581}")
        )

        if failures == 0 {
            fputs("decode_ahead_token_harness: ok\n", stderr)
            exit(0)
        }
        fputs("decode_ahead_token_harness: \(failures) failure(s)\n", stderr)
        exit(1)
    }
}

@MainActor var activeStreamSession: ActiveStreamSession?
@MainActor
@main
struct ParakeetSidecar {
    static func main() async {
        // Swift initializes globals lazily. Capture protocol stdout before a
        // first load/download command redirects it for native-library logging.
        // Otherwise the first progress event can permanently capture stderr.
        guard protocolStdoutFileDescriptor >= 0 else {
            log("Failed to preserve stdout for the sidecar protocol")
            exit(1)
        }
        logSystemInfo()

        if CommandLine.arguments.contains("--decode-ahead-token-harness") {
            DecodeAheadTokenNormalizationHarness.run()
            return
        }

        // Set up JSON encoder
        // IMPORTANT: Do NOT use .prettyPrinted - Rust parses line-by-line
        // Multi-line JSON will cause "EOF while parsing" errors
        let encoder = JSONEncoder()

        // Process command line arguments or stdin
        if CommandLine.arguments.count > 1 {
            // Direct file mode for testing
            let audioPath = CommandLine.arguments[1]
            await loadModel(version: .v3, forceDownload: false, emitStatus: false, encoder: encoder)
            await transcribeFile(audioPath, language: nil, translateToEnglish: false, encoder: encoder)
        } else {
            // JSON communication mode for Tauri
            await runEventLoop(encoder: encoder)
        }
    }

    static func runEventLoop(encoder: JSONEncoder) async {
        for await line in ProtocolStdin.lineStream() {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { continue }

            do {
                guard let data = trimmed.data(using: .utf8) else {
                    sendError("invalid_encoding", message: "Failed to parse command payload", encoder: encoder)
                    continue
                }

                guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    sendError("invalid_payload", message: "Command payload must be a JSON object", encoder: encoder)
                    continue
                }

                let commandType = json["type"] as? String
                if activeStreamSession != nil && isHeavyCommandBlockedDuringStream(commandType) {
                    sendError("stream_busy", message: "Parakeet streaming session is active; finish or cancel it before running this command", encoder: encoder)
                    continue
                }

                switch commandType {
                case "load_model", "download_model":
                    guard let model = parseSupportedModel(
                        modelId: json["model_id"],
                        modelVersion: json["model_version"]
                    ) else {
                        sendError("invalid_model", message: "Unsupported Parakeet model", encoder: encoder)
                        continue
                    }
                    let forceDownload = (json["force_download"] as? Bool)
                        ?? ((json["type"] as? String) == "download_model")
                    await loadSelectedModel(model, forceDownload: forceDownload, encoder: encoder)

                case "unload_model":
                    await unloadModel()
                    sendResponse(StatusResponse(loadedModel: nil, modelVersion: nil), encoder: encoder)

                case "delete_model":
                    guard let model = parseSupportedModel(
                        modelId: json["model_id"],
                        modelVersion: json["model_version"]
                    ) else {
                        sendError("invalid_model", message: "Unsupported Parakeet model", encoder: encoder)
                        continue
                    }
                    await deleteSelectedModel(model, encoder: encoder)

                case "transcribe":
                    if let audioPath = json["audio_path"] as? String {
                        let language = json["language"] as? String
                        let translateToEnglish = json["translate_to_english"] as? Bool ?? false
                        await transcribeFile(audioPath, language: language, translateToEnglish: translateToEnglish, encoder: encoder)
                    } else {
                        sendError("missing_audio_path", message: "audio_path is required", encoder: encoder)
                    }


                case "warmup":
                    await warmup(encoder: encoder)


                case "eou_model_status":
                    let chunkMs = json["chunk_ms"] as? Int ?? 320
                    await eouModelStatus(chunkMs: chunkMs, encoder: encoder)

                case "download_eou_model":
                    let chunkMs = json["chunk_ms"] as? Int ?? 320
                    await downloadEouModel(chunkMs: chunkMs, encoder: encoder)

                case "warmup_eou":
                    let chunkMs = json["chunk_ms"] as? Int ?? 320
                    await warmupEou(chunkMs: chunkMs, encoder: encoder)

                case "diarize":
                    if let audioPath = json["audio_path"] as? String {
                        await diarizeFile(audioPath, encoder: encoder)
                    } else {
                        sendError("missing_audio_path", message: "audio_path is required", encoder: encoder)
                    }

                case "start_stream":
                    await startStream(command: json, encoder: encoder)

                case "audio_chunk":
                    await receiveStreamAudioChunk(command: json, encoder: encoder)

                case "finalize_stream":
                    await finalizeStream(encoder: encoder)

                case "cancel_stream":
                    await cancelStream(encoder: encoder)

                case "status":
                    sendResponse(
                        StatusResponse(
                            loadedModel: loadedModel?.modelIdentifier,
                            modelVersion: loadedModel?.wireVersion
                        ),
                        encoder: encoder
                    )

                case "shutdown":
                    if activeStreamSession != nil {
                        await cancelStream(encoder: encoder, emitResponse: false)
                    }
                    await unloadModel()
                    exit(0)

                default:
                    sendError("unknown_command", message: "Unknown command type", encoder: encoder)
                }
            } catch {
                sendError("parse_error", message: "Failed to parse JSON: \(error)", encoder: encoder)
            }
        }
    }

    static func loadSelectedModel(
        _ model: SupportedModel,
        forceDownload: Bool,
        encoder: JSONEncoder
    ) async {
        if loadedModel == model, !forceDownload {
            sendResponse(
                StatusResponse(loadedModel: model.modelIdentifier, modelVersion: model.wireVersion),
                encoder: encoder
            )
            return
        }

        await unloadModel()
        switch model {
        case .tdt(let version):
            await loadModel(
                version: version,
                forceDownload: forceDownload,
                emitStatus: false,
                encoder: encoder
            )
            guard isModelLoaded else { return }
        case .unified640:
            do {
                let manager = StreamingUnifiedAsrManager(
                    configuration: nil,
                    config: UnifiedConfig(leftFrames: 70, chunkFrames: 7, rightFrames: 1),
                    encoderPrecision: .int8
                )
                if forceDownload {
                    deleteNativeModelFiles(for: model)
                    try await withLibraryStdoutRedirected {
                        try await manager.loadModels(
                            progressHandler: { progress in sendProgress(progress, encoder: encoder) }
                        )
                    }
                } else {
                    try await withLibraryStdoutRedirected {
                        try await manager.loadModels(from: nativeModelDirectory(for: model))
                    }
                }
                unifiedManager = manager
                isModelLoaded = true
            } catch {
                sendError(
                    "model_load_error",
                    message: "Failed to load Parakeet Unified: \(error.localizedDescription)",
                    encoder: encoder
                )
                return
            }
        case .nemotronMultilingual1120:
            do {
                let manager = StreamingNemotronMultilingualAsrManager()
                let directory: URL
                if forceDownload {
                    deleteNativeModelFiles(for: model)
                    directory = try await StreamingNemotronMultilingualAsrManager.downloadVariant(
                        languageCode: "multilingual",
                        chunkMs: 1120,
                        progressHandler: { progress in sendProgress(progress, encoder: encoder) }
                    )
                } else {
                    directory = nativeModelDirectory(for: model)
                }
                try await withLibraryStdoutRedirected {
                    try await manager.loadModels(from: directory)
                }
                nemotronMultilingualManager = manager
                isModelLoaded = true
            } catch {
                sendError(
                    "model_load_error",
                    message: "Failed to load Nemotron Multilingual: \(error.localizedDescription)",
                    encoder: encoder
                )
                return
            }
        }

        loadedModel = model
        sendResponse(
            StatusResponse(loadedModel: model.modelIdentifier, modelVersion: model.wireVersion),
            encoder: encoder
        )
    }

    static func deleteSelectedModel(_ model: SupportedModel, encoder: JSONEncoder) async {
        if loadedModel == model {
            await unloadModel()
        }
        switch model {
        case .tdt(let version):
            deleteModelFiles(for: version)
        case .unified640, .nemotronMultilingual1120:
            deleteNativeModelFiles(for: model)
        }
        sendResponse(
            StatusResponse(
                loadedModel: loadedModel?.modelIdentifier,
                modelVersion: loadedModel?.wireVersion
            ),
            encoder: encoder
        )
    }

    // VoiceTypr stores bare ISO-639 codes; the native bundle uses regional
    // prompt_dictionary keys for these languages.
    static func nemotronLanguageHint(_ language: String?) -> String {
        switch language?.lowercased() {
        case "af": return "af-ZA"
        case "am": return "am-ET"
        case "az": return "az-AZ"
        case "bn": return "bn-IN"
        case "fa": return "fa-IR"
        case "gu": return "gu-IN"
        case "ha": return "ha-NG"
        case "haw": return "haw-US"
        case "he": return "he-IL"
        case "hy": return "hy-AM"
        case "id": return "id-ID"
        case "ja": return "ja-JP"
        case "ka": return "ka-GE"
        case "km": return "km-KH"
        case "kn": return "kn-IN"
        case "ln": return "ln-CD"
        case "mi": return "mi-NZ"
        case "ml": return "ml-IN"
        case "mr": return "mr-IN"
        case "ms": return "ms-MY"
        case "mt": return "mt-MT"
        case "ne": return "ne-NP"
        case "si": return "si-LK"
        case "so": return "so-SO"
        case "sw": return "sw-KE"
        case "ta": return "ta-IN"
        case "te": return "te-IN"
        case "tg": return "tg-TJ"
        case "th": return "th-TH"
        case "ur": return "ur-PK"
        case "uz": return "uz-UZ"
        case "vi": return "vi-VN"
        case "yo": return "yo-NG"
        case "zh": return "zh-CN"
        case .some(let code): return code
        case nil: return "auto"
        }
    }

    static func voiceTyprLanguageCode(_ language: String?) -> String? {
        guard let language,
              let bareCode = language.split(
                maxSplits: 1,
                whereSeparator: { $0 == "-" || $0 == "_" }
              ).first else {
            return nil
        }
        return bareCode.lowercased()
    }

    static func nativeModelDirectory(for model: SupportedModel) -> URL {
        let root = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/FluidAudio/Models")
        switch model {
        case .unified640:
            return root.appendingPathComponent("parakeet-unified-en-0.6b", isDirectory: true)
        case .nemotronMultilingual1120:
            return root.appendingPathComponent(
                "nemotron-multilingual/multilingual/1120ms",
                isDirectory: true
            )
        case .tdt:
            return root
        }
    }

    static func deleteNativeModelFiles(for model: SupportedModel) {
        switch model {
        case .unified640:
            try? FileManager.default.removeItem(
                at: nativeModelDirectory(for: model)
                    .appendingPathComponent(
                        "parakeet_unified_encoder_streaming_70_7_1_int8.mlmodelc"
                    )
            )
        case .nemotronMultilingual1120:
            try? FileManager.default.removeItem(at: nativeModelDirectory(for: model))
        case .tdt:
            return
        }
    }

    static func loadModel(version: SupportedModelVersion = .v3, forceDownload: Bool = false, emitStatus: Bool = true, encoder: JSONEncoder) async {
        log("───────────────────────────────────────────────────────")
        log("🔄 LOAD MODEL REQUEST")
        log("───────────────────────────────────────────────────────")
        log("📦 Requested version: \(version.rawValue.uppercased()) (\(version.modelIdentifier))")
        log("📁 Repo folder: \(version.repoFolderName)")
        log("🔄 Force download: \(forceDownload)")
        log("📐 Running on: \(getArchitectureInfo())")

        // Check expected cache path
        let home = FileManager.default.homeDirectoryForCurrentUser
        let expectedPath = home
            .appendingPathComponent("Library/Application Support/FluidAudio/Models")
            .appendingPathComponent(version.repoFolderName)
        log("📍 Expected cache path: \(expectedPath.path)")
        log("📂 Path exists: \(FileManager.default.fileExists(atPath: expectedPath.path))")

        if FileManager.default.fileExists(atPath: expectedPath.path) {
            if let contents = try? FileManager.default.contentsOfDirectory(atPath: expectedPath.path) {
                log("📄 Cache contents: \(contents.joined(separator: ", "))")
            }
        }

        if isModelLoaded, !forceDownload, let loadedVersion = loadedModelVersion, loadedVersion == version {
            log("⚡ Model already loaded: \(loadedVersion.modelIdentifier)")
            if emitStatus {
                sendResponse(StatusResponse(loadedModel: loadedVersion.modelIdentifier, modelVersion: loadedVersion.rawValue), encoder: encoder)
            }
            return
        }

        do {
            let models: AsrModels
            let progressHandler: ProgressHandler = { progress in
                sendProgress(progress, encoder: encoder)
            }

            if forceDownload {
                log("📥 Force-downloading Parakeet \(version.rawValue.uppercased()) via FluidAudio...")
                log("🌐 This will download ~500MB. Please wait...")
                // FluidAudio's `downloadAndLoad` calls `download(force: false)`
                // internally, which silently reuses existing on-disk files (no
                // resume, no re-fetch). To honor `force_download`, first release
                // the in-memory model and purge every cache location for this
                // version (same paths as the `delete_model` handler) so the
                // subsequent download re-fetches a clean copy.
                if loadedModelVersion == version {
                    await unloadModel()
                }
                deleteModelFiles(for: version)
                models = try await withLibraryStdoutRedirected {
                    try await AsrModels.downloadAndLoad(version: version.asrVersion, progressHandler: progressHandler)
                }
                downloadedVersions.insert(version)
                log("✅ Download complete for \(version.rawValue.uppercased())")
            } else {
                log("🔍 Attempting to load Parakeet \(version.rawValue.uppercased()) from cache...")
                do {
                    models = try await withLibraryStdoutRedirected {
                        try await AsrModels.loadFromCache(version: version.asrVersion, progressHandler: progressHandler)
                    }
                    downloadedVersions.insert(version)
                    log("✅ Loaded Parakeet \(version.rawValue.uppercased()) from cache")
                } catch {
                    log("❌ Failed to load \(version.rawValue.uppercased()) from cache")
                    log("❌ Error type: \(type(of: error))")
                    log("❌ Error details: \(error)")
                    log("❌ Localized: \(error.localizedDescription)")
                    sendError("model_not_downloaded", message: "Parakeet \(version.rawValue.uppercased()) is not downloaded. Please download it first. Error: \(error.localizedDescription)", encoder: encoder)
                    return
                }
            }

            log("🔧 Initializing AsrManager...")
            let manager = AsrManager(config: .default)
            log("🔧 Calling manager.loadModels(_:)...")
            try await withLibraryStdoutRedirected {
                try await manager.loadModels(models)
            }
            log("✅ AsrManager initialized successfully")
            asrManager = manager
            loadedAsrModels = models

            isModelLoaded = true
            loadedModelVersion = version
            loadedModel = .tdt(version)
            log("✅ Model load complete: \(version.modelIdentifier)")
            if emitStatus {
                sendResponse(StatusResponse(loadedModel: version.modelIdentifier, modelVersion: version.rawValue), encoder: encoder)
            }
        } catch {
            log("❌ FATAL: Failed to load model \(version.rawValue.uppercased())")
            log("❌ Error type: \(type(of: error))")
            log("❌ Error details: \(error)")
            log("❌ Localized: \(error.localizedDescription)")
            sendError("model_load_error", message: "Failed to load model: \(error.localizedDescription)", encoder: encoder)
        }
        log("───────────────────────────────────────────────────────")
    }

    static func unloadModel() async {
        if activeStreamSession != nil {
            await cancelStream(encoder: JSONEncoder(), emitResponse: false)
        }
        await asrManager?.cleanup()
        await unifiedManager?.cleanup()
        await nemotronMultilingualManager?.cleanup()
        for manager in cachedEouManagers.values {
            await manager.cleanup()
        }
        cachedEouManagers.removeAll()
        asrManager = nil
        loadedAsrModels = nil
        unifiedManager = nil
        nemotronMultilingualManager = nil
        isModelLoaded = false
        loadedModelVersion = nil
        loadedModel = nil
    }

    static func deleteModelFiles(for version: SupportedModelVersion) {
        let fileManager = FileManager.default

        let home = fileManager.homeDirectoryForCurrentUser
        let targets: [URL] = [
            home
                .appendingPathComponent("Library/Application Support/FluidAudio/Models", isDirectory: true)
                .appendingPathComponent(version.repoFolderName, isDirectory: true),
            home
                .appendingPathComponent("Library/Application Support", isDirectory: true)
                .appendingPathComponent(version.repoFolderName, isDirectory: true),
            home
                .appendingPathComponent("Library/Caches/FluidAudio", isDirectory: true)
                .appendingPathComponent(version.repoFolderName, isDirectory: true)
        ]

        for path in targets {
            if fileManager.fileExists(atPath: path.path) {
                do {
                    try fileManager.removeItem(at: path)
                    log("🗑️  Deleted model files at: \(path.path)")
                } catch {
                    log("⚠️  Failed to delete model files at \(path.path): \(error)")
                }
            }
        }

        downloadedVersions.remove(version)
    }

    static func transcribeFile(
        _ audioPath: String,
        language: String? = nil,
        translateToEnglish: Bool = false,
        encoder: JSONEncoder
    ) async {
        guard isModelLoaded, let selectedModel = loadedModel else {
            sendError(
                "model_not_loaded",
                message: "Parakeet model not loaded. Please download it first from Settings.",
                encoder: encoder
            )
            return
        }
        guard FileManager.default.fileExists(atPath: audioPath) else {
            sendError("file_not_found", message: "Audio file not found: \(audioPath)", encoder: encoder)
            return
        }

        let fileURL = URL(fileURLWithPath: audioPath)
        do {
            let finalText: String
            let duration: Float
            let transcriptLanguage: String?
            switch selectedModel {
            case .tdt:
                guard let manager = asrManager else { throw ASRError.notInitialized }
                var decoderState = TdtDecoderState.make(
                    decoderLayers: await manager.decoderLayerCount
                )
                let result = try await withLibraryStdoutRedirected {
                    try await manager.transcribe(fileURL, decoderState: &decoderState)
                }
                finalText = result.text
                duration = Float(result.duration)
                transcriptLanguage = language

            case .unified640:
                guard let manager = unifiedManager else { throw ASRError.notInitialized }
                let samples = try AudioConverter().resampleAudioFile(fileURL)
                guard let buffer = makeFloatPcmBuffer(samples) else {
                    throw ASRError.invalidAudioData
                }
                try await manager.reset()
                await manager.setPartialTranscriptCallback { _ in }
                try await manager.appendAudio(buffer)
                try await manager.processBufferedAudio()
                finalText = try await manager.finish()
                duration = Float(samples.count) / 16_000
                transcriptLanguage = "en"

            case .nemotronMultilingual1120:
                guard let manager = nemotronMultilingualManager else {
                    throw ASRError.notInitialized
                }
                let samples = try AudioConverter().resampleAudioFile(fileURL)
                await manager.reset()
                await manager.setLanguage(nemotronLanguageHint(language))
                await manager.setPartialCallback { _ in }
                _ = try await manager.process(samples: samples)
                finalText = try await manager.finish()
                duration = Float(samples.count) / 16_000
                transcriptLanguage = voiceTyprLanguageCode(
                    await manager.detectedLanguage()
                ) ?? language
            }

            if translateToEnglish {
                log("⚠️ Parakeet translation is not supported; returning transcription")
            }
            sendResponse(
                TranscriptionResponse(
                    text: finalText,
                    segments: [],
                    language: transcriptLanguage,
                    duration: duration
                ),
                encoder: encoder
            )
        } catch {
            sendError(
                "transcription_failed",
                message: "Transcription failed: \(error.localizedDescription)",
                encoder: encoder
            )
        }
    }

    static func warmup(encoder: JSONEncoder) async {
        log("───────────────────────────────────────────────────────")
        log("🔥 WARMUP REQUEST")
        log("───────────────────────────────────────────────────────")

        let startTime = Date()
        var warmupURL: URL?

        func finish(warmed: Bool, error: String? = nil) {
            let elapsedMs = Int(Date().timeIntervalSince(startTime) * 1000.0)
            if warmed {
                log("✅ Warmup complete in \(elapsedMs)ms")
            } else if let error {
                log("⚠️ Warmup skipped/failed after \(elapsedMs)ms: \(error)")
            }
            sendResponse(WarmedResponse(warmed: warmed, ms: elapsedMs, error: error), encoder: encoder)
        }

        guard isModelLoaded else {
            finish(warmed: false, error: "Parakeet model is not loaded")
            return
        }

        do {
            switch loadedModel {
            case .tdt:
                guard let manager = asrManager else { throw ASRError.notInitialized }
                let fileURL = try writeWarmupSilenceWav()
                warmupURL = fileURL
                var decoderState = TdtDecoderState.make(
                    decoderLayers: await manager.decoderLayerCount
                )
                _ = try await withLibraryStdoutRedirected {
                    try await manager.transcribe(fileURL, decoderState: &decoderState)
                }
            case .unified640:
                guard let manager = unifiedManager,
                      let buffer = makeFloatPcmBuffer(Array(repeating: 0, count: 16_000))
                else { throw ASRError.notInitialized }
                try await manager.reset()
                try await manager.appendAudio(buffer)
                try await manager.processBufferedAudio()
                _ = try await manager.finish()
            case .nemotronMultilingual1120:
                guard let manager = nemotronMultilingualManager,
                      let buffer = makeFloatPcmBuffer(Array(repeating: 0, count: 16_000))
                else { throw ASRError.notInitialized }
                await manager.reset()
                await manager.setLanguage("auto")
                _ = try await manager.process(audioBuffer: buffer)
                _ = try await manager.finish()
            case nil:
                throw ASRError.notInitialized
            }
            finish(warmed: true)
        } catch {
            finish(warmed: false, error: error.localizedDescription)
        }

        if let warmupURL {
            do {
                try FileManager.default.removeItem(at: warmupURL)
            } catch {
                log("⚠️ Failed to delete warmup wav: \(error.localizedDescription)")
            }
        }

        log("───────────────────────────────────────────────────────")
    }

    static func writeWarmupSilenceWav() throws -> URL {
        let fileURL = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("voicetypr-parakeet-warmup-\(UUID().uuidString).wav")
        let sampleRate: UInt32 = 16_000
        let channels: UInt16 = 1
        let bitsPerSample: UInt16 = 32
        let formatCode: UInt16 = 3 // IEEE float
        let durationSeconds: UInt32 = 1
        let samples = Int(sampleRate * durationSeconds)
        let dataBytes = UInt32(samples * MemoryLayout<Float32>.size)
        let byteRate = sampleRate * UInt32(channels) * UInt32(bitsPerSample / 8)
        let blockAlign = channels * (bitsPerSample / 8)

        var data = Data()
        data.append(contentsOf: "RIFF".utf8)
        appendUInt32LE(36 + dataBytes, to: &data)
        data.append(contentsOf: "WAVE".utf8)
        data.append(contentsOf: "fmt ".utf8)
        appendUInt32LE(16, to: &data)
        appendUInt16LE(formatCode, to: &data)
        appendUInt16LE(channels, to: &data)
        appendUInt32LE(sampleRate, to: &data)
        appendUInt32LE(byteRate, to: &data)
        appendUInt16LE(blockAlign, to: &data)
        appendUInt16LE(bitsPerSample, to: &data)
        data.append(contentsOf: "data".utf8)
        appendUInt32LE(dataBytes, to: &data)
        data.append(Data(repeating: 0, count: Int(dataBytes)))

        try data.write(to: fileURL, options: .atomic)
        return fileURL
    }

    static func appendUInt32LE(_ value: UInt32, to data: inout Data) {
        var little = value.littleEndian
        withUnsafeBytes(of: &little) { data.append(contentsOf: $0) }
    }

    static func appendUInt16LE(_ value: UInt16, to data: inout Data) {
        var little = value.littleEndian
        withUnsafeBytes(of: &little) { data.append(contentsOf: $0) }
    }

    static func isHeavyCommandBlockedDuringStream(_ commandType: String?) -> Bool {
        switch commandType {
        case "load_model", "download_model", "unload_model", "delete_model", "transcribe", "download_eou_model", "warmup", "warmup_eou", "diarize", "start_stream":
            return true
        default:
            return false
        }
    }

    static func streamingConfig(from command: [String: Any]) -> SlidingWindowAsrConfig {
        let rawConfig = command["config"]
        let base: SlidingWindowAsrConfig
        if let configName = rawConfig as? String, configName == "default" {
            base = .default
        } else {
            base = .streaming
        }

        guard let object = rawConfig as? [String: Any] else {
            return base
        }

        let chunkSeconds = object["chunk_seconds"] as? Double ?? base.chunkSeconds
        let hypothesisChunkSeconds = object["hypothesis_chunk_seconds"] as? Double ?? base.hypothesisChunkSeconds
        let leftContextSeconds = object["left_context_seconds"] as? Double ?? base.leftContextSeconds
        let rightContextSeconds = object["right_context_seconds"] as? Double ?? base.rightContextSeconds
        let minContextForConfirmation = object["min_context_for_confirmation"] as? Double ?? base.minContextForConfirmation
        let confirmationThreshold = object["confirmation_threshold"] as? Double ?? base.confirmationThreshold

        return SlidingWindowAsrConfig(
            chunkSeconds: chunkSeconds,
            hypothesisChunkSeconds: hypothesisChunkSeconds,
            leftContextSeconds: leftContextSeconds,
            rightContextSeconds: rightContextSeconds,
            minContextForConfirmation: minContextForConfirmation,
            confirmationThreshold: confirmationThreshold
        )
    }

    static func doubleValue(_ value: Any?) -> Double? {
        if let double = value as? Double {
            return double
        }
        if let int = value as? Int {
            return Double(int)
        }
        return nil
    }

    static func eouChunkSize(from chunkMs: Int) -> StreamingChunkSize? {
        switch chunkMs {
        case 160:
            return .ms160
        case 320:
            return .ms320
        case 1280:
            return .ms1280
        default:
            return nil
        }
    }

    static func eouRepo(from chunkMs: Int) -> Repo? {
        switch chunkMs {
        case 160:
            return .parakeetEou160
        case 320:
            return .parakeetEou320
        case 1280:
            return .parakeetEou1280
        default:
            return nil
        }
    }

    static func eouRepoFolderName(chunkMs: Int) -> String? {
        eouRepo(from: chunkMs)?.folderName
    }

    static func eouModelsRootDirectory() -> URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("FluidAudio", isDirectory: true)
            .appendingPathComponent("Models", isDirectory: true)
            .appendingPathComponent("parakeet-eou-streaming", isDirectory: true)
    }

    static func eouCanonicalModelDirectory(chunkMs: Int) -> URL? {
        guard let root = eouModelsRootDirectory(),
              let repoFolderName = eouRepoFolderName(chunkMs: chunkMs) else {
            return nil
        }
        return root.appendingPathComponent(repoFolderName, isDirectory: true)
    }

    static func eouLegacyFlatModelDirectory(chunkMs: Int) -> URL? {
        guard eouChunkSize(from: chunkMs) != nil else {
            return nil
        }
        return FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("FluidAudio", isDirectory: true)
            .appendingPathComponent("Models", isDirectory: true)
            .appendingPathComponent("parakeet-eou-streaming", isDirectory: true)
            .appendingPathComponent("\(chunkMs)ms", isDirectory: true)
    }

    static func eouRequiredModelsExist(in directory: URL) -> Bool {
        let required = [
            "streaming_encoder.mlmodelc",
            "decoder.mlmodelc",
            "joint_decision.mlmodelc",
            "vocab.json",
        ]
        return required.allSatisfy { name in
            FileManager.default.fileExists(atPath: directory.appendingPathComponent(name).path)
        }
    }

    static func eouResolvedModelDirectory(chunkMs: Int) -> URL? {
        guard let canonical = eouCanonicalModelDirectory(chunkMs: chunkMs) else {
            return nil
        }
        if eouRequiredModelsExist(in: canonical) {
            return canonical
        }
        if let legacy = eouLegacyFlatModelDirectory(chunkMs: chunkMs),
           eouRequiredModelsExist(in: legacy) {
            return legacy
        }
        return canonical
    }

    static func eouModelDownloaded(chunkMs: Int) -> (downloaded: Bool, path: String?) {
        guard let directory = eouResolvedModelDirectory(chunkMs: chunkMs) else {
            return (false, nil)
        }
        let downloaded = eouRequiredModelsExist(in: directory)
        return (downloaded, downloaded ? directory.path : nil)
    }

    static func eouModelStatus(chunkMs: Int, encoder: JSONEncoder) async {
        guard eouChunkSize(from: chunkMs) != nil else {
            sendError("invalid_chunk_ms", message: "chunk_ms must be 160, 320, or 1280", encoder: encoder)
            return
        }
        let status = eouModelDownloaded(chunkMs: chunkMs)
        sendResponse(
            EouModelStatusResponse(chunkMs: chunkMs, downloaded: status.downloaded, path: status.path),
            encoder: encoder
        )
    }

    static func loadCachedEouManager(chunkMs: Int) async throws -> StreamingEouAsrManager {
        if let manager = cachedEouManagers[chunkMs] {
            return manager
        }
        guard let chunkSize = eouChunkSize(from: chunkMs),
              let directory = eouResolvedModelDirectory(chunkMs: chunkMs) else {
            throw NSError(domain: "VoicetyprParakeet", code: 1, userInfo: [NSLocalizedDescriptionKey: "chunk_ms must be 160, 320, or 1280"])
        }
        let status = eouModelDownloaded(chunkMs: chunkMs)
        guard status.downloaded else {
            throw NSError(domain: "VoicetyprParakeet", code: 2, userInfo: [NSLocalizedDescriptionKey: "Parakeet EOU \(chunkMs)ms model is not downloaded"])
        }
        let manager = StreamingEouAsrManager(chunkSize: chunkSize)
        try await withLibraryStdoutRedirected {
            try await manager.loadModels(from: directory)
        }
        cachedEouManagers[chunkMs] = manager
        return manager
    }

    static func downloadEouModel(chunkMs: Int, encoder: JSONEncoder) async {
        guard let chunkSize = eouChunkSize(from: chunkMs) else {
            sendError("invalid_chunk_ms", message: "chunk_ms must be 160, 320, or 1280", encoder: encoder)
            return
        }
        do {
            let manager = StreamingEouAsrManager(chunkSize: chunkSize)
            let progressHandler: ProgressHandler = { progress in
                sendProgress(progress, encoder: encoder)
            }
            guard let rootDirectory = eouModelsRootDirectory() else {
                sendError("eou_model_download_failed", message: "Failed to resolve EOU model cache directory", encoder: encoder)
                return
            }
            try await withLibraryStdoutRedirected {
                try await manager.loadModels(to: rootDirectory, configuration: nil, progressHandler: progressHandler)
            }
            cachedEouManagers[chunkMs] = manager
            sendResponse(OkResponse(command: "download_eou_model"), encoder: encoder)
        } catch {
            sendError("eou_model_download_failed", message: "Failed to download EOU model: \(error.localizedDescription)", encoder: encoder)
        }
    }

    static func makeSilenceBuffer(sampleRate: Double = 16_000, durationSeconds: Double = 1.0) -> AVAudioPCMBuffer? {
        let frames = AVAudioFrameCount(sampleRate * durationSeconds)
        guard let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: sampleRate, channels: 1, interleaved: false),
              let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames) else {
            return nil
        }
        buffer.frameLength = frames
        return buffer
    }

    static func warmupEou(chunkMs: Int, encoder: JSONEncoder) async {
        let startTime = Date()
        do {
            let manager = try await loadCachedEouManager(chunkMs: chunkMs)
            guard let buffer = makeSilenceBuffer() else {
                sendResponse(WarmedResponse(warmed: false, ms: 0, error: "Failed to create warmup buffer"), encoder: encoder)
                return
            }
            await manager.reset()
            try await manager.appendAudio(buffer)
            try await manager.processBufferedAudio()
            _ = try await manager.finish()
            let elapsedMs = Int(Date().timeIntervalSince(startTime) * 1000.0)
            sendResponse(WarmedResponse(warmed: true, ms: elapsedMs, error: nil), encoder: encoder)
        } catch {
            let elapsedMs = Int(Date().timeIntervalSince(startTime) * 1000.0)
            sendResponse(WarmedResponse(warmed: false, ms: elapsedMs, error: error.localizedDescription), encoder: encoder)
        }
    }

    static func startStream(command: [String: Any], encoder: JSONEncoder) async {
        guard activeStreamSession == nil else {
            sendError("stream_busy", message: "A Parakeet stream is already active", encoder: encoder)
            return
        }
        guard isModelLoaded, let selectedModel = loadedModel else {
            sendError("model_not_loaded", message: "Parakeet model must be loaded before streaming", encoder: encoder)
            return
        }
        if let requestedModel = command["model_id"] as? String,
           requestedModel != selectedModel.modelIdentifier {
            sendError(
                "model_mismatch",
                message: "Loaded model is \(selectedModel.modelIdentifier), requested \(requestedModel)",
                encoder: encoder
            )
            return
        }

        let sampleRate = doubleValue(command["sample_rate"]) ?? 0
        let channels = command["channels"] as? Int ?? 0
        guard sampleRate > 0, channels > 0 else {
            sendError("invalid_stream_format", message: "start_stream requires positive sample_rate and channels", encoder: encoder)
            return
        }

        let engine = (command["engine"] as? String) ?? "sliding_window"
        guard selectedModel.supportsStreamEngine(engine) else {
            sendError(
                "stream_engine_mismatch",
                message: "Stream engine \(engine) is incompatible with \(selectedModel.modelIdentifier)",
                encoder: encoder
            )
            return
        }

        do {
            switch engine {
            case "eou":
                let chunkMs = command["chunk_ms"] as? Int ?? 320
                let manager = try await loadCachedEouManager(chunkMs: chunkMs)
                try await withLibraryStdoutRedirected {
                    await manager.reset()
                }
                let session = ActiveStreamSession(
                    engine: .eou(manager),
                    sampleRate: sampleRate,
                    channels: channels,
                    encoder: encoder
                )
                await manager.setPartialCallback { transcript in
                    Task { @MainActor in
                        guard let active = activeStreamSession else { return }
                        guard transcript.hasPrefix(active.committedPrefix) else {
                            log("⚠️ Dropping non-monotonic EOU tentative stream text")
                            return
                        }
                        active.latestPartial = transcript
                        let tentative = String(transcript.dropFirst(active.committedPrefix.count))
                        sendResponse(
                            StreamPartialResponse(text: tentative, isConfirmed: false, confidence: 0.0),
                            encoder: encoder
                        )
                    }
                }
                await manager.setEouCallback { transcript in
                    Task { @MainActor in
                        guard let active = activeStreamSession else { return }
                        guard transcript.hasPrefix(active.committedPrefix) else {
                            log("⚠️ Dropping non-monotonic EOU committed stream text")
                            return
                        }
                        active.committedPrefix = transcript
                        active.latestPartial = transcript
                        sendResponse(
                            StreamPartialResponse(text: transcript, isConfirmed: true, confidence: 1.0),
                            encoder: encoder
                        )
                    }
                }
                activeStreamSession = session
                sendResponse(StreamStartedResponse(), encoder: encoder)

            case "sliding_window":
                guard let models = loadedAsrModels else {
                    sendError("model_not_loaded", message: "TDT models are not loaded", encoder: encoder)
                    return
                }
                let manager = SlidingWindowAsrManager(config: streamingConfig(from: command))
                try await withLibraryStdoutRedirected {
                    try await manager.loadModels(models)
                    try await manager.startStreaming(source: .microphone)
                }
                let session = ActiveStreamSession(
                    engine: .slidingWindow(manager),
                    sampleRate: sampleRate,
                    channels: channels,
                    encoder: encoder
                )
                session.forwarder = Task {
                    for await update in await manager.transcriptionUpdates {
                        sendResponse(
                            StreamPartialResponse(
                                text: update.text,
                                isConfirmed: update.isConfirmed,
                                confidence: update.confidence
                            ),
                            encoder: encoder
                        )
                    }
                }
                activeStreamSession = session
                sendResponse(StreamStartedResponse(), encoder: encoder)

            case "decode_ahead":
                guard let sharedManager = asrManager else {
                    sendError("model_not_loaded", message: "Parakeet engine is not initialized for decode_ahead streaming", encoder: encoder)
                    return
                }
                let decoderLayers = await sharedManager.decoderLayerCount
                let session = DecodeAheadAsrSession(manager: sharedManager, decoderLayers: decoderLayers, encoder: encoder)
                let activeSession = ActiveStreamSession(
                    engine: .decodeAhead(session),
                    sampleRate: sampleRate,
                    channels: channels,
                    encoder: encoder
                )
                activeStreamSession = activeSession
                sendResponse(StreamStartedResponse(), encoder: encoder)

            case "unified_english":
                guard let manager = unifiedManager else {
                    sendError("model_not_loaded", message: "Parakeet Unified is not loaded", encoder: encoder)
                    return
                }
                try await manager.reset()
                await manager.setPartialTranscriptCallback { transcript in
                    sendResponse(
                        StreamPartialResponse(text: transcript, isConfirmed: false, confidence: 0.0),
                        encoder: encoder
                    )
                }
                activeStreamSession = ActiveStreamSession(
                    engine: .unified(manager),
                    sampleRate: sampleRate,
                    channels: channels,
                    encoder: encoder
                )
                sendResponse(StreamStartedResponse(), encoder: encoder)

            case "nemotron_multilingual":
                guard let manager = nemotronMultilingualManager else {
                    sendError("model_not_loaded", message: "Nemotron Multilingual is not loaded", encoder: encoder)
                    return
                }
                await manager.reset()
                // Streaming preview auto-detects language; the authoritative batch
                // final still receives the user's selected language hint.
                await manager.setLanguage("auto")
                await manager.setPartialCallback { transcript in
                    sendResponse(
                        StreamPartialResponse(text: transcript, isConfirmed: false, confidence: 0.0),
                        encoder: encoder
                    )
                }
                activeStreamSession = ActiveStreamSession(
                    engine: .nemotronMultilingual(manager),
                    sampleRate: sampleRate,
                    channels: channels,
                    encoder: encoder
                )
                sendResponse(StreamStartedResponse(), encoder: encoder)

            default:
                sendError("invalid_stream_engine", message: "Unsupported stream engine", encoder: encoder)
            }
        } catch {
            sendError("stream_start_failed", message: "Failed to start stream: \(error.localizedDescription)", encoder: encoder)
        }
    }

    static func receiveStreamAudioChunk(command: [String: Any], encoder: JSONEncoder) async {
        guard let session = activeStreamSession else {
            sendError("stream_not_active", message: "No active stream session", encoder: encoder)
            return
        }
        guard let pcmBase64 = command["pcm_b64"] as? String,
              let data = Data(base64Encoded: pcmBase64) else {
            sendError("invalid_audio_chunk", message: "audio_chunk requires base64 pcm_b64", encoder: encoder)
            return
        }
        guard let buffer = makePcmBuffer(
            fromLittleEndianI16: data,
            sampleRate: session.sampleRate,
            channels: session.channels
        ) else {
            sendError("invalid_audio_chunk", message: "audio_chunk payload is not aligned to i16 channels", encoder: encoder)
            return
        }

        switch session.engine {
        case .slidingWindow(let manager):
            do {
                try await withLibraryStdoutRedirected {
                    await manager.streamAudio(buffer)
                }
            } catch {
                sendError("stream_chunk_failed", message: "Failed to process stream chunk: \(error.localizedDescription)", encoder: encoder)
            }
        case .eou(let manager):
            do {
                try await withLibraryStdoutRedirected {
                    try await manager.appendAudio(buffer)
                    try await manager.processBufferedAudio()
                    let toks = await manager.getRawTokenStrings()
                    log("DBG-EOU frames=\(buffer.frameLength) tokens=\(toks.count) sample='\(toks.suffix(5).joined(separator: "|"))'")
                }
            } catch {
                sendError("stream_chunk_failed", message: "Failed to process stream chunk: \(error.localizedDescription)", encoder: encoder)
            }
        case .decodeAhead(let decodeSession):
            session.enqueueDecodeAheadChunk(buffer, decodeSession: decodeSession)
        case .unified(let manager):
            do {
                try await manager.appendAudio(buffer)
                try await manager.processBufferedAudio()
            } catch {
                sendError("stream_chunk_failed", message: "Failed to process Unified stream chunk: \(error.localizedDescription)", encoder: encoder)
            }
        case .nemotronMultilingual(let manager):
            do {
                _ = try await manager.process(audioBuffer: buffer)
            } catch {
                sendError("stream_chunk_failed", message: "Failed to process Nemotron stream chunk: \(error.localizedDescription)", encoder: encoder)
            }
        }
        // Fire-and-forget command: no response on success.
    }

    static func finalizeStream(encoder: JSONEncoder) async {
        guard let session = activeStreamSession else {
            sendError("stream_not_active", message: "No active stream session", encoder: encoder)
            return
        }
        activeStreamSession = nil

        do {
            let finalText: String
            switch session.engine {
            case .slidingWindow(let manager):
                finalText = try await withLibraryStdoutRedirected {
                    try await manager.finish()
                }
            case .eou(let manager):
                finalText = try await withLibraryStdoutRedirected {
                    try await manager.finish()
                }
            case .decodeAhead(let decodeSession):
                finalText = await session.finalizeDecodeAhead(decodeSession)
            case .unified(let manager):
                finalText = try await manager.finish()
            case .nemotronMultilingual(let manager):
                finalText = try await manager.finish()
            }
            session.forwarder?.cancel()
            sendResponse(StreamFinalResponse(text: finalText), encoder: encoder)
        } catch {
            session.forwarder?.cancel()
            sendError("stream_finalize_failed", message: "Failed to finalize stream: \(error.localizedDescription)", encoder: encoder)
        }
    }

    static func cancelStream(encoder: JSONEncoder, emitResponse: Bool = true) async {
        guard let session = activeStreamSession else {
            if emitResponse {
                sendResponse(StreamCancelledResponse(), encoder: encoder)
            }
            return
        }
        activeStreamSession = nil
        do {
            switch session.engine {
            case .slidingWindow(let manager):
                try await withLibraryStdoutRedirected {
                    await manager.cancel()
                }
            case .eou(let manager):
                try await withLibraryStdoutRedirected {
                    await manager.reset()
                }
            case .decodeAhead(let decodeSession):
                await session.cancelDecodeAheadImmediately(decodeSession)
            case .unified(let manager):
                try await manager.reset()
            case .nemotronMultilingual(let manager):
                await manager.reset()
            }
        } catch {
            log("⚠️ Stream cancel cleanup failed: \(error.localizedDescription)")
        }
        session.forwarder?.cancel()
        if emitResponse {
            sendResponse(StreamCancelledResponse(), encoder: encoder)
        }
    }

    static func makePcmBuffer(
        fromLittleEndianI16 data: Data,
        sampleRate: Double,
        channels: Int
    ) -> AVAudioPCMBuffer? {
        guard channels > 0, data.count % (MemoryLayout<Int16>.size * channels) == 0 else {
            return nil
        }
        let frameCount = data.count / (MemoryLayout<Int16>.size * channels)
        guard frameCount > 0,
              let format = AVAudioFormat(
                commonFormat: .pcmFormatFloat32,
                sampleRate: sampleRate,
                channels: AVAudioChannelCount(channels),
                interleaved: false
              ),
              let buffer = AVAudioPCMBuffer(
                pcmFormat: format,
                frameCapacity: AVAudioFrameCount(frameCount)
              ),
              let channelData = buffer.floatChannelData else {
            return nil
        }

        data.withUnsafeBytes { rawBuffer in
            let bytes = rawBuffer.bindMemory(to: UInt8.self)
            for frame in 0..<frameCount {
                for channel in 0..<channels {
                    let sampleIndex = (frame * channels + channel) * 2
                    let raw = UInt16(bytes[sampleIndex]) | (UInt16(bytes[sampleIndex + 1]) << 8)
                    let signed = Int16(bitPattern: raw)
                    channelData[channel][frame] = Float(signed) / Float(Int16.max)
                }
            }
        }
        buffer.frameLength = AVAudioFrameCount(frameCount)
        return buffer
    }

    static func makeFloatPcmBuffer(_ samples: [Float]) -> AVAudioPCMBuffer? {
        guard !samples.isEmpty,
              let format = AVAudioFormat(
                commonFormat: .pcmFormatFloat32,
                sampleRate: 16_000,
                channels: 1,
                interleaved: false
              ),
              let buffer = AVAudioPCMBuffer(
                pcmFormat: format,
                frameCapacity: AVAudioFrameCount(samples.count)
              ),
              let channel = buffer.floatChannelData?[0] else {
            return nil
        }
        samples.withUnsafeBufferPointer { source in
            channel.update(from: source.baseAddress!, count: samples.count)
        }
        buffer.frameLength = AVAudioFrameCount(samples.count)
        return buffer
    }


    nonisolated static func diarizeFile(_ audioPath: String, encoder: JSONEncoder) async {
        log("───────────────────────────────────────────────────────")
        log("👥 DIARIZATION REQUEST")
        log("───────────────────────────────────────────────────────")
        log("📄 Audio path: \(audioPath)")

        guard FileManager.default.fileExists(atPath: audioPath) else {
            log("❌ Audio file not found: \(audioPath)")
            sendError("file_not_found", message: "Audio file not found: \(audioPath)", encoder: encoder)
            return
        }

        do {
            let manager = OfflineDiarizerManager()
            try await manager.prepareModels()
            let result = try await manager.process(URL(fileURLWithPath: audioPath))
            let segments = result.segments.map { segment in
                SpeakerSegment(
                    speakerId: segment.speakerId,
                    start: segment.startTimeSeconds,
                    end: segment.endTimeSeconds
                )
            }

            sendResponse(DiarizationResponse(segments: segments), encoder: encoder)
        } catch {
            log("❌ DIARIZATION FAILED")
            log("❌ Error type: \(type(of: error))")
            log("❌ Error details: \(error)")
            log("❌ Localized: \(error.localizedDescription)")
            sendError("diarization_failed", message: "Diarization failed: \(error.localizedDescription)", encoder: encoder)
        }

        log("───────────────────────────────────────────────────────")
    }

    nonisolated static func sendResponse<T: Encodable>(_ response: T, encoder: JSONEncoder) {
        do {
            let data = try encoder.encode(response)
            if let jsonString = String(data: data, encoding: .utf8) {
                writeProtocolLine(jsonString)
            }
        } catch {
            writeProtocolLine("{\"type\":\"error\",\"code\":\"serialization_error\",\"message\":\"Failed to serialize response\"}")
        }
    }

    nonisolated static func sendError(_ code: String, message: String, encoder: JSONEncoder) {
        sendResponse(ErrorResponse(code: code, message: message), encoder: encoder)
    }

    nonisolated static func sendProgress(_ progress: DownloadProgress, encoder: JSONEncoder) {
        let phase: String
        switch progress.phase {
        case .listing:
            phase = "listing"
        case .downloading(let completedFiles, let totalFiles):
            phase = "downloading \(completedFiles)/\(totalFiles)"
        case .compiling(let modelName):
            phase = modelName.isEmpty ? "compiling" : "compiling \(modelName)"
        }

        sendResponse(
            ProgressResponse(
                progress: max(0.0, min(1.0, progress.fractionCompleted)),
                phase: phase
            ),
            encoder: encoder
        )
    }


    static func parseSupportedModel(modelId: Any?, modelVersion: Any?) -> SupportedModel? {
        switch (modelId as? String)?.lowercased() {
        case "parakeet-unified-640ms":
            return .unified640
        case "nemotron-multilingual-1120ms":
            return .nemotronMultilingual1120
        default:
            return parseModelVersion(modelVersion).map(SupportedModel.tdt)
        }
    }

    static func parseModelVersion(_ value: Any?) -> SupportedModelVersion? {
        guard let str = (value as? String)?.lowercased() else {
            return nil
        }

        switch str {
        case "v2":
            return .v2
        case "v3":
            return .v3
        default:
            return nil
        }
    }
}
