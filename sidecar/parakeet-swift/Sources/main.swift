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



struct IncomingVocabularyTerm: Decodable {
    let text: String
    let aliases: [String]

    private enum CodingKeys: String, CodingKey {
        case text, aliases
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        text = try container.decode(String.self, forKey: .text)
        aliases = try container.decodeIfPresent([String].self, forKey: .aliases) ?? []
    }
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
    let customVocabularySupported: Bool = true
    let customVocabularyReady: Bool = ctcVocabularyReady()
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
@MainActor var cachedCtcModels: CtcModels?
@MainActor var cachedCtcTokenizer: CtcTokenizer?

func ctcVocabularyReady() -> Bool {
    let directory = CtcModels.defaultCacheDirectory(for: .ctc110m)
    let tokenizerURL = directory.appendingPathComponent("tokenizer.json")
    return CtcModels.modelsExist(at: directory)
        && FileManager.default.fileExists(atPath: tokenizerURL.path)
}
@MainActor var cachedCtcSpotter: CtcKeywordSpotter?
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
    var decodeAheadDrainTask: Task<Void, Never>?
    var decodeAheadNoMoreInput = false

    init(engine: Engine, sampleRate: Double, channels: Int, encoder: JSONEncoder) {
        self.engine = engine
        self.sampleRate = sampleRate
        self.channels = channels
        self.encoder = encoder
    }

    func enqueueDecodeAheadChunk(_ buffer: AVAudioPCMBuffer, decodeSession: DecodeAheadAsrSession) async {
        guard !decodeAheadNoMoreInput else {
            log("⚠️ decode_ahead: ignoring audio_chunk after no-more-input")
            return
        }
        await decodeSession.ingestSamples(buffer)
        ensureDecodeAheadDrain(decodeSession: decodeSession)
    }

    private func ensureDecodeAheadDrain(decodeSession: DecodeAheadAsrSession) {
        guard !decodeAheadNoMoreInput, decodeAheadDrainTask == nil else { return }
        decodeAheadDrainTask = Task { @MainActor in
            while !Task.isCancelled && !self.decodeAheadNoMoreInput {
                guard await decodeSession.runLiveDecodeIfNeeded() else { break }
            }
            self.decodeAheadDrainTask = nil
            if !self.decodeAheadNoMoreInput, await decodeSession.needsLiveDecode() {
                self.ensureDecodeAheadDrain(decodeSession: decodeSession)
            }
        }
    }

    func finalizeDecodeAhead(_ decodeSession: DecodeAheadAsrSession) async -> String {
        decodeAheadNoMoreInput = true
        if let drain = decodeAheadDrainTask {
            await drain.value
        }
        decodeAheadDrainTask = nil
        return await decodeSession.finalize()
    }

    func cancelDecodeAheadImmediately(_ decodeSession: DecodeAheadAsrSession) async {
        decodeAheadDrainTask?.cancel()
        decodeAheadNoMoreInput = true
        decodeAheadDrainTask = nil
        await decodeSession.cancel()
    }
}

struct DecodeAheadPlanner {
    static let sampleRate = 16_000
    static let minSamples = 16_000
    static let stepSamples = 8_000
    static let maxWindow = 224_000
    static let maxModelSamples = 240_000
    // Tuned on real speech 2026-09-27: 3 s of left context let a slid German window
    // flip to English ("Ingenieur und Kapitän" -> "engineer on Capitaine"); 8 s keeps
    // the language and topic. Decode cost is flat up to the 15 s model input.
    static let slideTrigger = 216_000
    static let leftContext = 128_000
    static let tailMargin = 32_000
    static let forceCommitAge = 96_000
    static let timeTolerance = 2_560
    static let blankSlideStep = 16_000
    static let maxPreviewSamples = 120 * sampleRate

    struct Token {
        let text: String
        let startTime: Double
        let endTime: Double
    }

    struct Word {
        let text: String
        let normKey: String
        let startAbs: Int
        let endAbs: Int
        let pieces: [String]
    }

    struct Window {
        let samples: [Float]
        let startAbs: Int
        let endAbs: Int
    }

    struct Update {
        let grewCommitted: Bool
        let committed: String
        let tentative: String
    }

    private(set) var samples: [Float] = []
    private(set) var baseOffset = 0
    private(set) var windowStart = 0
    private(set) var committedEnd = 0
    private(set) var committed = ""
    private(set) var history: [[Word]] = []
    private(set) var lastDecodeEnd = 0
    private var lastAttemptWindowStart = -1
    private var lastAttemptInputEnd = -1
    private(set) var didDropPreviewAudio = false
    private var tentative = ""
    private var committedWords: [Word] = []
    private var lastSuccessfulWords: [Word] = []
    private var lastSuccessfulText = ""
    // A slide freezes the prefix before its left-context boundary. Final decode
    // replaces everything committed after that boundary.
    private var finalPrefix = ""
    private var finalPrefixWords: [Word] = []
    private var finalBoundary = 0
    private var finalized = false

    var end: Int { baseOffset + samples.count }
    @discardableResult
    mutating func append(_ newSamples: [Float]) -> Bool {
        samples.append(contentsOf: newSamples)
        guard samples.count > Self.maxPreviewSamples else { return false }
        // Only this preview buffer is trimmed. The recording WAV and batch
        // transcription retain their complete audio independently.
        windowStart = max(windowStart, end - Self.maxPreviewSamples)
        captureFinalBoundary()
        compact()
        let firstDrop = !didDropPreviewAudio
        didDropPreviewAudio = true
        return firstDrop
    }

    func needsLiveDecode() -> Bool {
        let availableEnd = min(end, windowStart + Self.maxWindow)
        guard availableEnd - windowStart >= Self.minSamples else { return false }
        if availableEnd == lastDecodeEnd {
            return end > lastAttemptInputEnd && windowStart == lastAttemptWindowStart
        }
        return availableEnd - lastDecodeEnd >= Self.stepSamples
            || (availableEnd == windowStart + Self.maxWindow && availableEnd > lastDecodeEnd)
    }

    mutating func nextWindow(finalizing: Bool = false) -> Window? {
        if finalizing {
            guard !finalized, end > 0 else { return nil }
            if end <= Self.maxModelSamples {
                guard baseOffset == 0 else { return nil }
                return Window(samples: samples, startAbs: 0, endAbs: end)
            }
            let finalStart = max(end - Self.maxModelSamples, finalBoundary - Self.leftContext)
            if finalStart >= baseOffset && finalStart <= windowStart
                && end - windowStart <= Self.maxWindow {
                let start = finalStart - baseOffset
                return Window(samples: Array(samples[start...]), startAbs: finalStart, endAbs: end)
            }
        } else {
            guard needsLiveDecode() else { return nil }
        }
        let windowEnd = min(end, windowStart + Self.maxWindow)
        guard windowStart != lastAttemptWindowStart || windowEnd != lastDecodeEnd
            || end > lastAttemptInputEnd else { return nil }
        let start = windowStart - baseOffset
        let stop = windowEnd - baseOffset
        guard start >= 0, stop > start, stop <= samples.count else { return nil }
        return Window(samples: Array(samples[start..<stop]), startAbs: windowStart, endAbs: windowEnd)
    }

    mutating func decodeFailed(window: Window, atInputEnd inputEnd: Int) {
        recordAttempt(window, inputEnd: inputEnd)
        if window.endAbs - window.startAbs >= Self.maxWindow {
            boundedCapSlide(window.endAbs)
        }
    }

    private mutating func recordAttempt(_ window: Window, inputEnd: Int) {
        lastAttemptWindowStart = window.startAbs
        lastDecodeEnd = window.endAbs
        lastAttemptInputEnd = inputEnd
    }

    mutating func apply(
        tokens: [Token],
        text: String,
        modelReturnedTimings: Bool,
        window: Window,
        draining: Bool = false,
        final: Bool = false,
        atInputEnd inputEnd: Int? = nil
    ) -> Update {
        guard !finalized else { return Update(grewCommitted: false, committed: committed, tentative: "") }
        recordAttempt(window, inputEnd: inputEnd ?? end)
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && (tokens.isEmpty || !modelReturnedTimings) {
            // FluidAudio already retries whole-window blanks internally. A blank
            // supplies no new hypothesis or commit evidence.
            if window.endAbs - window.startAbs >= Self.maxWindow { boundedCapSlide(window.endAbs) }
            return Update(grewCommitted: false, committed: committed, tentative: tentative)
        }
        let before = committed
        if !modelReturnedTimings || tokens.isEmpty {
            let fallback = text.trimmingCharacters(in: .whitespacesAndNewlines)
            lastSuccessfulText = fallback
            lastSuccessfulWords = []
            if final {
                committed = end <= Self.maxModelSamples ? "" : finalPrefix
                committed += Self.normalizedText(fallback, leadingContent: !committed.isEmpty)
                finalized = true
            } else if draining || window.endAbs - windowStart >= Self.maxWindow {
                let ownedText = window.startAbs >= committedEnd ? fallback : fallbackSuffix(fallback)
                committed += Self.normalizedText(ownedText, leadingContent: !committed.isEmpty)
                committedEnd = max(committedEnd, window.endAbs)
                // Timing-less text owns its entire decoded window. Retaining overlap
                // would let a later decode append the same audio a second time.
                windowStart = window.endAbs
                finalPrefix = committed
                finalPrefixWords = committedWords.filter { $0.endAbs <= windowStart }
                finalBoundary = windowStart
                lastSuccessfulText = ""
                history.removeAll()
                compact()
            }
            if draining && windowStart != window.endAbs {
                advanceDrain(window)
            } else if !final && windowStart != window.endAbs {
                slide(decodedEnd: window.endAbs, remaining: [])
                history.append([])
                if history.count > 2 { history.removeFirst() }
            }
            tentative = final || draining || windowStart == window.endAbs ? "" :
                (window.startAbs >= committedEnd ? fallback : fallbackSuffix(fallback))
            return Update(
                grewCommitted: committed != before,
                committed: committed,
                tentative: tentative
            )
        }

        let boundary = final ? finalBoundary : committedEnd
        let filtered = tokens.compactMap { token -> (String, Int, Int)? in
            guard token.startTime.isFinite, token.endTime.isFinite,
                  !token.text.isEmpty, token.text != "<blank>", token.text != "<pad>" else { return nil }
            let start = window.startAbs + Int((token.startTime * Double(Self.sampleRate)).rounded())
            let end = window.startAbs + Int((token.endTime * Double(Self.sampleRate)).rounded())
            guard (final && window.endAbs <= Self.maxModelSamples)
                || start >= finalBoundary - Self.timeTolerance else { return nil }
            return (token.text, start, max(start, end))
        }
        let wholeWords = Array(filtered.drop(while: { !Self.startsWord($0.0) }))
        let allWords = Self.groupWords(wholeWords)
        lastSuccessfulWords = allWords.filter { $0.endAbs > finalBoundary }
        let words: [Word]
        if final && end <= Self.maxModelSamples {
            words = allWords
        } else {
            words = Self.dropBoundaryDuplicates(
                allWords, committedWords: final ? finalPrefixWords : committedWords,
                boundary: boundary
            )
        }
        lastSuccessfulText = allWords.isEmpty ? text.trimmingCharacters(in: .whitespacesAndNewlines) : ""
        if final {
            let prefix = end <= Self.maxModelSamples ? "" : finalPrefix
            committed = prefix + Self.detokenize(words.flatMap(\.pieces), leadingContent: !prefix.isEmpty)
            finalized = true
            tentative = ""
            return Update(grewCommitted: committed != before, committed: committed, tentative: "")
        }
        var count = 0
        if draining {
            count = words.prefix { $0.endAbs <= window.endAbs - Self.tailMargin }.count
        } else {
            for (index, word) in words.enumerated() {
                guard word.endAbs <= window.endAbs - Self.tailMargin,
                      history.count == 2,
                      history.allSatisfy({ prior in Self.agreesWithHistory(
                          word, at: index, in: words, prior: prior
                      ) }) else { break }
                count += 1
            }
        }
        commit(Array(words.prefix(count)), next: words.dropFirst(count).first)
        if draining {
            advanceDrain(window)
        } else {
            slide(decodedEnd: window.endAbs, remaining: Array(words.dropFirst(count)),
                hadProgress: committed != before)
            history.append(Array(words.dropFirst(count)))
            if history.count > 2 { history.removeFirst() }
        }
        tentative = draining ? "" : Self.detokenize(
            words.drop(while: { $0.endAbs <= committedEnd }).flatMap(\.pieces),
            leadingContent: !committed.isEmpty
        )
        return Update(grewCommitted: committed != before, committed: committed, tentative: tentative)
    }

    private mutating func slide(decodedEnd: Int, remaining: [Word], hadProgress: Bool = false) {
        guard decodedEnd - windowStart > Self.slideTrigger else { return }
        var remaining = remaining
        if decodedEnd - windowStart >= Self.maxWindow && !hadProgress {
            let forced = Array(remaining.prefix { $0.endAbs <= decodedEnd - Self.forceCommitAge })
            commit(forced, next: remaining.dropFirst(forced.count).first)
            remaining.removeFirst(forced.count)
            if forced.isEmpty {
                boundedCapSlide(decodedEnd)
                return
            }
        }
        let previousStart = windowStart
        windowStart = max(windowStart, max(0, committedEnd - Self.leftContext))
        if decodedEnd - windowStart >= Self.maxWindow {
            let forced = Array(remaining.prefix { $0.endAbs <= decodedEnd - Self.forceCommitAge })
            commit(forced, next: remaining.dropFirst(forced.count).first)
            windowStart = max(windowStart, max(0, committedEnd - Self.leftContext))
            if decodedEnd - windowStart >= Self.maxWindow {
                let oldest = remaining.dropFirst(forced.count).first?.startAbs ?? decodedEnd
                let contextual = min(decodedEnd - Self.leftContext, oldest - Self.leftContext)
                windowStart = max(windowStart, contextual > windowStart ? contextual : oldest)
            }
        }
        if windowStart > previousStart { captureFinalBoundary() }
        compact()
    }

    private mutating func boundedCapSlide(_ decodedEnd: Int) {
        let earliestUncommitted = lastSuccessfulWords
            .filter { $0.endAbs > committedEnd }.map(\.startAbs).min() ?? decodedEnd
        let newStart = min(windowStart + Self.blankSlideStep, earliestUncommitted)
        guard newStart > windowStart else { return }
        windowStart = newStart
        captureFinalBoundary()
        compact()
    }

    private mutating func advanceDrain(_ window: Window) {
        let previousStart = windowStart
        windowStart = max(windowStart, window.endAbs - Self.leftContext)
        if windowStart > previousStart { captureFinalBoundary() }
        compact()
    }

    private mutating func captureFinalBoundary() {
        let newlyFrozen = committedWords.filter {
            $0.endAbs > finalBoundary && $0.startAbs < windowStart
        }
        finalPrefix += Self.detokenize(newlyFrozen.flatMap(\.pieces), leadingContent: !finalPrefix.isEmpty)
        finalPrefixWords.append(contentsOf: newlyFrozen)
        if let last = newlyFrozen.last { finalBoundary = last.endAbs }
    }

    private mutating func compact() {
        let retainedStart = min(windowStart, max(0, end - Self.maxModelSamples))
        let remove = retainedStart - baseOffset
        if remove > 0 {
            samples.removeFirst(remove)
            baseOffset = retainedStart
        }
    }

    private mutating func commit(_ words: [Word], next: Word?) {
        guard let last = words.last else { return }
        committed += Self.detokenize(words.flatMap(\.pieces), leadingContent: !committed.isEmpty)
        committedWords.append(contentsOf: words)
        let gap = max(0, (next?.startAbs ?? last.endAbs) - last.endAbs)
        committedEnd = max(committedEnd, last.endAbs + gap / 2)
    }

    mutating func finalizeFromLastHypothesis() -> String {
        guard !finalized else { return committed }
        if !lastSuccessfulWords.isEmpty {
            let remaining = lastSuccessfulWords.filter { $0.endAbs > finalBoundary }
            let words = Self.dropBoundaryDuplicates(
                remaining, committedWords: finalPrefixWords, boundary: finalBoundary
            )
            committed = finalPrefix + Self.detokenize(words.flatMap(\.pieces), leadingContent: !finalPrefix.isEmpty)
        } else if !lastSuccessfulText.isEmpty {
            committed = finalPrefix + Self.normalizedText(lastSuccessfulText, leadingContent: !finalPrefix.isEmpty)
        } else {
            committed = finalPrefix
        }
        tentative = ""
        finalized = true
        return committed
    }

    private func fallbackSuffix(_ text: String) -> String {
        let incoming = text.split(whereSeparator: \.isWhitespace).map(String.init)
        let prior = committed.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !incoming.isEmpty else { return "" }
        let limit = min(incoming.count, prior.count)
        if limit > 0 {
            for overlap in stride(from: limit, through: 1, by: -1) {
                let left = prior.suffix(overlap).map(Self.normKey)
                let right = incoming.prefix(overlap).map(Self.normKey)
                if left == right { return incoming.dropFirst(overlap).joined(separator: " ") }
            }
        }
        return incoming.joined(separator: " ")
    }

    private static func groupWords(_ tokens: [(String, Int, Int)]) -> [Word] {
        var groups: [Word] = []
        var pieces: [String] = []
        var start = 0
        var end = 0
        func flush() {
            guard !pieces.isEmpty else { return }
            let rendered = detokenize(pieces, leadingContent: false)
            groups.append(Word(text: rendered, normKey: normKey(rendered), startAbs: start, endAbs: end, pieces: pieces))
            pieces = []
        }
        for (piece, tokenStart, tokenEnd) in tokens {
            if startsWord(piece) && !pieces.isEmpty { flush() }
            if pieces.isEmpty {
                start = tokenStart
                end = tokenEnd
            } else {
                end = max(end, tokenEnd)
            }
            pieces.append(piece)
        }
        flush()
        return groups
    }

    private static func dropBoundaryDuplicates(
        _ words: [Word], committedWords: [Word], boundary: Int
    ) -> [Word] {
        let tail = Array(committedWords.suffix(4))
        let limit = min(words.count, tail.count)
        if limit > 0 {
            for overlap in stride(from: limit, through: 1, by: -1) {
                let previous = Array(tail.suffix(overlap))
                for start in words.indices where start + overlap <= words.count {
                    let incoming = Array(words[start..<(start + overlap)])
                    guard zip(previous, incoming).allSatisfy({ old, new in
                        !old.normKey.isEmpty && old.normKey == new.normKey
                    }) else { continue }
                    // A lone later occurrence of the same short word is new speech.
                    // A touching or overlapping span can still be the committed word
                    // even when its duration has shifted between hypotheses.
                    if overlap == 1 && incoming[0].startAbs > previous[0].endAbs + Self.sampleRate / 40 {
                        continue
                    }
                    if incoming[0].startAbs <= boundary + Self.timeTolerance {
                        return Array(words.dropFirst(start + overlap)).filter { $0.endAbs > boundary }
                    }
                }
            }
        }
        let grouped = words.filter { $0.endAbs > boundary }
        let rawLimit = min(grouped.count, committedWords.count)
        if rawLimit > 0 {
            for overlap in stride(from: rawLimit, through: 1, by: -1) {
                if zip(committedWords.suffix(overlap), grouped.prefix(overlap)).allSatisfy({ old, new in
                    !old.normKey.isEmpty && old.normKey == new.normKey && sameOccurrence(old, new)
                }) {
                    return Array(grouped.dropFirst(overlap))
                }
            }
        }
        return grouped
    }

    private static func agreesWithHistory(_ word: Word, at index: Int, in words: [Word], prior: [Word]) -> Bool {
        guard !word.normKey.isEmpty else { return false }
        let context = min(4, index + 1, prior.count)
        guard context > 0 else { return false }
        for length in stride(from: context, through: 1, by: -1) {
            let current = words[(index - length + 1)...index].map(\.normKey)
            for priorEnd in prior.indices where priorEnd >= length - 1 {
                let earlier = prior[(priorEnd - length + 1)...priorEnd].map(\.normKey)
                if current == earlier && priorEnd == index
                    && abs(prior[priorEnd].startAbs - word.startAbs) <= Self.timeTolerance {
                    return true
                }
            }
        }
        return prior.contains { $0.normKey == word.normKey && sameOccurrence($0, word) }
    }

    private static func sameOccurrence(_ old: Word, _ new: Word) -> Bool {
        let overlap = min(old.endAbs, new.endAbs) - max(old.startAbs, new.startAbs)
        let shorter = min(old.endAbs - old.startAbs, new.endAbs - new.startAbs)
        return overlap > 0 && shorter > 0 && overlap * 2 >= shorter
    }

    private static func normKey(_ text: String) -> String {
        text.folding(options: [.caseInsensitive], locale: Locale(identifier: "en_US_POSIX"))
            .filter { $0.isLetter || $0.isNumber }
    }

    private static func startsWord(_ piece: String) -> Bool {
        piece.hasPrefix("\u{2581}") || piece.hasPrefix(" ")
    }

    private static func detokenize(_ pieces: [String], leadingContent: Bool) -> String {
        var result = ""
        var hasContent = leadingContent
        for piece in pieces {
            if piece.isEmpty || piece == "<blank>" || piece == "<pad>" { continue }
            if startsWord(piece) {
                let rest = piece.hasPrefix("\u{2581}")
                    ? String(piece.dropFirst())
                    : String(piece.dropFirst()).trimmingCharacters(in: .whitespaces)
                if hasContent { result += " " }
                if !rest.isEmpty {
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

    private static func normalizedText(_ text: String, leadingContent: Bool) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        return leadingContent ? " " + trimmed : trimmed
    }
}

actor DecodeAheadAsrSession {
    private struct DecodeAheadDecodeResult {
        enum Outcome {
            case failure
            case success(timings: [TokenTiming], text: String, modelReturnedTimings: Bool)
        }
        let outcome: Outcome
    }

    private var planner = DecodeAheadPlanner()
    private var emissionGeneration: UInt64 = 0
    private let converter = AudioConverter()
    private weak var manager: AsrManager?
    private let decoderLayers: Int
    private let languageHint: Language?
    private let encoder: JSONEncoder

    init(manager: AsrManager, decoderLayers: Int, language: String?, encoder: JSONEncoder) {
        self.manager = manager
        self.decoderLayers = decoderLayers
        self.languageHint = Self.languageHint(for: language)
        self.encoder = encoder
    }

    static func languageHint(for language: String?) -> Language? {
        language.flatMap(Language.init(rawValue:))
    }

    func ingestSamples(_ buffer: AVAudioPCMBuffer) async {
        guard let resampled = try? converter.resampleBuffer(buffer), !resampled.isEmpty else {
            log("⚠️ decode_ahead: failed to resample audio chunk; skipping")
            return
        }
        if planner.append(resampled) {
            log("⚠️ decode_ahead: preview backlog exceeded 120 s; dropping oldest preview audio")
        }
    }

    func needsLiveDecode() -> Bool { manager != nil && planner.needsLiveDecode() }

    func runLiveDecodeIfNeeded() async -> Bool {
        guard !Task.isCancelled, let manager = manager,
              let window = planner.nextWindow() else { return false }
        let attemptInputEnd = planner.end
        let generation = emissionGeneration
        let result = await Self.decode(
            manager: manager, window: window.samples, decoderLayers: decoderLayers, languageHint: languageHint
        )
        guard !Task.isCancelled, emissionGeneration == generation else { return false }
        switch result.outcome {
        case .failure:
            planner.decodeFailed(window: window, atInputEnd: attemptInputEnd)
            return true
        case .success(let timings, let text, let modelReturnedTimings):
            let update = planner.apply(
                tokens: Self.plannerTokens(timings), text: text,
                modelReturnedTimings: modelReturnedTimings, window: window,
                atInputEnd: attemptInputEnd
            )
            if update.grewCommitted {
                ParakeetSidecar.sendResponse(
                    StreamPartialResponse(text: update.committed, isConfirmed: true, confidence: 1.0), encoder: encoder
                )
            }
            ParakeetSidecar.sendResponse(
                StreamPartialResponse(text: update.tentative, isConfirmed: false, confidence: 0.0), encoder: encoder
            )
            return true
        }
    }

    func finalize() async -> String {
        guard let manager = manager else { return planner.finalizeFromLastHypothesis() }
        let generation = emissionGeneration
        let maxPasses = planner.end / DecodeAheadPlanner.blankSlideStep + 4
        var passes = 0
        while passes < maxPasses, let window = planner.nextWindow(finalizing: true) {
            passes += 1
            let attemptInputEnd = planner.end
            let result = await Self.decode(
                manager: manager, window: window.samples, decoderLayers: decoderLayers, languageHint: languageHint
            )
            guard !Task.isCancelled, emissionGeneration == generation else { break }
            guard case .success(let timings, let text, let modelReturnedTimings) = result.outcome else {
                planner.decodeFailed(window: window, atInputEnd: attemptInputEnd)
                if window.endAbs == planner.end || planner.windowStart == window.startAbs { break }
                continue
            }
            let final = window.endAbs == planner.end
            let blank = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                && (timings.isEmpty || !modelReturnedTimings)
            _ = planner.apply(
                tokens: Self.plannerTokens(timings), text: text,
                modelReturnedTimings: modelReturnedTimings, window: window,
                draining: !final, final: final && !blank, atInputEnd: attemptInputEnd
            )
            if blank {
                if final || planner.windowStart == window.startAbs { break }
            }
            if final { break }
        }
        return planner.finalizeFromLastHypothesis()
    }

    func cancel() {
        emissionGeneration &+= 1
        planner = DecodeAheadPlanner()
    }

    private static func plannerTokens(_ timings: [TokenTiming]) -> [DecodeAheadPlanner.Token] {
        timings.map { DecodeAheadPlanner.Token(text: $0.token, startTime: $0.startTime, endTime: $0.endTime) }
    }

    @MainActor
    private static func decode(
        manager: AsrManager,
        window: [Float],
        decoderLayers: Int,
        languageHint: Language?
    ) async -> DecodeAheadDecodeResult {
        var state = TdtDecoderState.make(decoderLayers: decoderLayers)
        do {
            let result = try await withLibraryStdoutRedirected {
                try await manager.transcribe(window, decoderState: &state, language: languageHint)
            }
            let text = result.text
            if let timings = result.tokenTimings {
                return DecodeAheadDecodeResult(outcome: .success(timings: timings, text: text, modelReturnedTimings: true))
            }
            return DecodeAheadDecodeResult(outcome: .success(timings: [], text: text, modelReturnedTimings: false))
        } catch {
            log("⚠️ decode_ahead: transcribe failed: \(error.localizedDescription)")
            return DecodeAheadDecodeResult(outcome: .failure)
        }
    }
}

enum DecodeAheadV2Harness {
    private static func pass(
        _ planner: inout DecodeAheadPlanner,
        endSeconds: Double,
        words: [(String, Double, Double)],
        text: String = "",
        timings: Bool = true,
        final: Bool = false
    ) -> DecodeAheadPlanner.Update {
        let target = Int((endSeconds * Double(DecodeAheadPlanner.sampleRate)).rounded())
        planner.append(Array(repeating: Float(0), count: max(0, target - planner.end)))
        guard let window = planner.nextWindow(finalizing: final) else {
            return DecodeAheadPlanner.Update(grewCommitted: false, committed: planner.committed, tentative: "")
        }
        let tokens = words.map { word in
            DecodeAheadPlanner.Token(
                text: word.0,
                startTime: word.1 - Double(window.startAbs) / Double(DecodeAheadPlanner.sampleRate),
                endTime: word.2 - Double(window.startAbs) / Double(DecodeAheadPlanner.sampleRate)
            )
        }
        return planner.apply(tokens: tokens, text: text, modelReturnedTimings: timings, window: window, final: final)
    }

    static func run() {
        var failures = 0
        func check(_ name: String, _ ok: Bool, detail: String = "") {
            let failureDetail = !ok && !detail.isEmpty ? " — \(detail)" : ""
            fputs("\(ok ? "PASS" : "FAIL") \(name)\(failureDetail)\n", stderr)
            if !ok { failures += 1 }
        }

        let one = ("▁One", 2.0, 2.4)
        var agreement = DecodeAheadPlanner()
        let first = pass(&agreement, endSeconds: 5.0, words: [one])
        let second = pass(&agreement, endSeconds: 5.5, words: [one])
        let third = pass(&agreement, endSeconds: 6.0, words: [one])
        check("three_hypothesis_agreement", first.committed.isEmpty && second.committed.isEmpty && third.committed == "One")
        let context = pass(&agreement, endSeconds: 6.5, words: [one, ("▁Two", 4.0, 4.4)])
        check("left_context_tokens_dropped", context.tentative == " Two" && context.committed == "One")

        var continuation = DecodeAheadPlanner()
        let missed = ("▁missed", 1.0, 1.4)
        _ = pass(&continuation, endSeconds: 5.0, words: [missed])
        _ = pass(&continuation, endSeconds: 5.5, words: [missed])
        _ = pass(&continuation, endSeconds: 6.0, words: [missed])
        let noFragment = pass(&continuation, endSeconds: 6.5, words: [
            ("ed", 1.45, 1.65), ("▁Next", 2.0, 2.4)
        ])
        check("continuation_piece_never_starts_kept_text", noFragment.committed == "missed" && noFragment.tentative == " Next")

        var duplicate = DecodeAheadPlanner()
        let has = ("▁has", 1.0, 1.4)
        _ = pass(&duplicate, endSeconds: 5.0, words: [has])
        _ = pass(&duplicate, endSeconds: 5.5, words: [has])
        _ = pass(&duplicate, endSeconds: 6.0, words: [has])
        let noDuplicate = pass(&duplicate, endSeconds: 6.5, words: [
            ("▁has", 1.3, 1.7), ("▁Next", 2.0, 2.4)
        ])
        // Sequence alignment identifies the shifted "has" as the committed occurrence.
        check("boundary_partial_overlap_kept", noDuplicate.committed == "has" && noDuplicate.tentative == " Next")

        var repetition = DecodeAheadPlanner()
        let very = ("▁very", 1.0, 1.2)
        _ = pass(&repetition, endSeconds: 5.0, words: [very])
        _ = pass(&repetition, endSeconds: 5.5, words: [very])
        _ = pass(&repetition, endSeconds: 6.0, words: [very])
        let repeated = pass(&repetition, endSeconds: 6.5, words: [("▁very", 1.4, 1.6)])
        check("genuine_repetition_kept", repeated.committed == "very" && repeated.tentative == " very")

        var shortRepetition = DecodeAheadPlanner()
        let shortFirst = ("▁go", 1.00, 1.08)
        _ = pass(&shortRepetition, endSeconds: 5.0, words: [shortFirst])
        _ = pass(&shortRepetition, endSeconds: 5.5, words: [shortFirst])
        _ = pass(&shortRepetition, endSeconds: 6.0, words: [shortFirst])
        let shortRepeated = pass(&shortRepetition, endSeconds: 6.5, words: [("▁go", 1.16, 1.24)])
        check("genuine_short_repetition_kept", shortRepeated.committed == "go"
            && shortRepeated.tentative == " go",
            detail: "committed=\(String(reflecting: shortRepeated.committed)), tentative=\(String(reflecting: shortRepeated.tentative)), committedEnd=\(shortRepetition.committedEnd)")

        var jittered = DecodeAheadPlanner()
        _ = pass(&jittered, endSeconds: 5.0, words: [("▁go", 1.00, 1.08)])
        _ = pass(&jittered, endSeconds: 5.5, words: [("▁go", 1.08, 1.16)])
        let jitterCommitted = pass(&jittered, endSeconds: 6.0, words: [("▁go", 1.16, 1.24)])
        let jitterAfterBoundary = pass(&jittered, endSeconds: 6.5, words: [("▁go", 1.24, 1.32)])
        check("jittered_short_word_not_duplicated", jitterCommitted.committed == "go"
            && jitterAfterBoundary.committed == "go" && jitterAfterBoundary.tentative.isEmpty)

        var sequenceRepetition = DecodeAheadPlanner()
        let firstGo = [("▁let's", 0.7, 0.9), ("▁go", 1.0, 1.08)]
        _ = pass(&sequenceRepetition, endSeconds: 5.0, words: firstGo)
        _ = pass(&sequenceRepetition, endSeconds: 5.5, words: firstGo)
        _ = pass(&sequenceRepetition, endSeconds: 6.0, words: firstGo)
        let secondGo = pass(&sequenceRepetition, endSeconds: 6.5, words: firstGo + [("▁go", 1.16, 1.24)])
        check("genuine_repetition_kept_by_sequence", secondGo.committed == "let's go"
            && secondGo.tentative == " go")

        var cap = DecodeAheadPlanner()
        _ = pass(&cap, endSeconds: 13.76, words: [])
        cap.append(Array(repeating: Float(0), count: DecodeAheadPlanner.maxWindow - cap.end))
        let capScheduled = cap.needsLiveDecode()
        let capWindow = cap.nextWindow()
        if let capWindow {
            _ = cap.apply(tokens: [], text: "", modelReturnedTimings: true, window: capWindow)
        }
        let capRetry = cap.nextWindow()
        check("cap_decode_is_never_starved", capScheduled && capWindow?.endAbs == DecodeAheadPlanner.maxWindow
            && cap.windowStart == DecodeAheadPlanner.blankSlideStep && capRetry?.startAbs == nil
            && cap.lastDecodeEnd == DecodeAheadPlanner.maxWindow)

        var failed = DecodeAheadPlanner()
        _ = pass(&failed, endSeconds: 13.76, words: [])
        failed.append(Array(repeating: Float(0), count: DecodeAheadPlanner.maxWindow - failed.end))
        let failedWindow = failed.nextWindow()
        let failedInputEnd = failed.end
        if let failedWindow { failed.decodeFailed(window: failedWindow, atInputEnd: failedInputEnd) }
        let noImmediateRetry = !failed.needsLiveDecode()
            && failed.lastDecodeEnd == DecodeAheadPlanner.maxWindow
        failed.append(Array(repeating: Float(0), count: DecodeAheadPlanner.stepSamples))
        let retriesWithAudio = failed.needsLiveDecode()
        if let retryWindow = failed.nextWindow() {
            _ = failed.apply(tokens: [], text: "", modelReturnedTimings: true, window: retryWindow)
        }
        check("failed_decode_does_not_wedge", failedWindow?.endAbs == DecodeAheadPlanner.maxWindow
            && noImmediateRetry && retriesWithAudio
            && failed.windowStart == DecodeAheadPlanner.blankSlideStep && !failed.needsLiveDecode())

        var failedTail = DecodeAheadPlanner()
        failedTail.append(Array(repeating: 0, count: 18 * DecodeAheadPlanner.sampleRate))
        let firstFailedCap = failedTail.nextWindow(finalizing: true)
        if let firstFailedCap { failedTail.decodeFailed(window: firstFailedCap, atInputEnd: failedTail.end) }
        let afterFailure = failedTail.nextWindow(finalizing: true)
        if let afterFailure {
            _ = failedTail.apply(tokens: [.init(text: "▁Transient", startTime: 13.0,
                endTime: 13.2)], text: "Transient", modelReturnedTimings: true,
                window: afterFailure, draining: true)
        }
        let tailWindow = failedTail.nextWindow(finalizing: true)
        var recoveredTail = ""
        if let tailWindow {
            let tailStart = Double(17 * DecodeAheadPlanner.sampleRate - tailWindow.startAbs)
                / Double(DecodeAheadPlanner.sampleRate)
            recoveredTail = failedTail.apply(tokens: [.init(text: "▁Tail", startTime: tailStart,
                endTime: tailStart + 0.4)], text: "Tail", modelReturnedTimings: true,
                window: tailWindow, final: true).committed
        }
        check("failed_cap_window_still_finalizes_tail", firstFailedCap?.endAbs == DecodeAheadPlanner.maxWindow
            && afterFailure?.startAbs == DecodeAheadPlanner.blankSlideStep
            && tailWindow?.endAbs == failedTail.end && recoveredTail == "Tail",
            detail: "failed=\(firstFailedCap?.endAbs ?? -1), after=\(afterFailure?.startAbs ?? -1), tail=\(tailWindow?.startAbs ?? -1)..\(tailWindow?.endAbs ?? -1), end=\(failedTail.end), text=\(recoveredTail)")

        var blank = DecodeAheadPlanner()
        let retained = ("▁Near", 10.5, 11.0)
        let tentativeBeforeBlank = pass(&blank, endSeconds: 13.5, words: [retained])
        blank.append(Array(repeating: Float(0), count: DecodeAheadPlanner.maxWindow - blank.end))
        let blankWindow = blank.nextWindow()
        let blankUpdate = blankWindow.map {
            blank.apply(tokens: [], text: "", modelReturnedTimings: true, window: $0)
        }
        check("blank_decode_keeps_uncommitted_audio", tentativeBeforeBlank.tentative == "Near"
            && blankUpdate?.tentative == "Near" && blankUpdate?.committed.isEmpty == true
            && blank.windowStart <= Int((retained.1 * Double(DecodeAheadPlanner.sampleRate)).rounded())
            && blank.windowStart == DecodeAheadPlanner.blankSlideStep
            && blank.committedEnd == 0 && blank.history.count == 1)

        var blankThenUncommittable = DecodeAheadPlanner()
        blankThenUncommittable.append(Array(repeating: Float(0), count: DecodeAheadPlanner.maxWindow))
        let capBlankWindow = blankThenUncommittable.nextWindow(finalizing: true)
        if let capBlankWindow {
            _ = blankThenUncommittable.apply(tokens: [], text: "", modelReturnedTimings: true,
                window: capBlankWindow)
        }
        let shiftedWindow = blankThenUncommittable.nextWindow(finalizing: true)
        if let shiftedWindow {
            _ = blankThenUncommittable.apply(tokens: [DecodeAheadPlanner.Token(
                text: "▁Late", startTime: 12.5, endTime: 12.9
            )], text: "Late", modelReturnedTimings: true, window: shiftedWindow)
        }
        let blankFallback = blankThenUncommittable.finalizeFromLastHypothesis()
        // A short session now retries final from sample zero; its blank falls back once.
        check("blank_at_cap_never_loops", capBlankWindow?.endAbs == DecodeAheadPlanner.maxWindow
            && shiftedWindow?.startAbs == 0 && blankFallback == "Late"
            && blankThenUncommittable.nextWindow(finalizing: true) == nil)

        var finalizingBlank = DecodeAheadPlanner()
        finalizingBlank.append(Array(repeating: 0, count: DecodeAheadPlanner.maxWindow))
        var blankFinalAttempts = 0
        while let window = finalizingBlank.nextWindow(finalizing: true), blankFinalAttempts < 4 {
            blankFinalAttempts += 1
            let final = window.endAbs == finalizingBlank.end
            _ = finalizingBlank.apply(tokens: [], text: "", modelReturnedTimings: true, window: window,
                draining: !final, final: final)
            if final { break }
        }
        check("finalize_exits_while_blank", blankFinalAttempts == 1
            && finalizingBlank.finalizeFromLastHypothesis().isEmpty)

        var wordBoundary = DecodeAheadPlanner()
        let crossing = ("▁Crossing", 0.5, 1.5)
        _ = pass(&wordBoundary, endSeconds: 5.0, words: [crossing])
        _ = pass(&wordBoundary, endSeconds: 5.5, words: [crossing])
        _ = pass(&wordBoundary, endSeconds: 6.0, words: [crossing])
        _ = pass(&wordBoundary, endSeconds: 14.0, words: [])
        let wordBoundaryFinal = wordBoundary.finalizeFromLastHypothesis()
        var uncommittedBoundary = DecodeAheadPlanner()
        let uncommitted = ("▁Waiting", 0.75, 1.25)
        _ = pass(&uncommittedBoundary, endSeconds: 13.5, words: [uncommitted])
        _ = pass(&uncommittedBoundary, endSeconds: 14.0, words: [])
        check("bounded_slide_never_cuts_a_word", wordBoundary.windowStart == DecodeAheadPlanner.blankSlideStep
            && wordBoundaryFinal == "Crossing"
            && uncommittedBoundary.windowStart == Int(0.75 * Double(DecodeAheadPlanner.sampleRate)))

        var grouped = DecodeAheadPlanner()
        let pieces = [("▁trans", 1.0, 1.1), ("cript", 1.1, 1.4), (" world", 2.0, 2.4)]
        _ = pass(&grouped, endSeconds: 5.0, words: pieces)
        _ = pass(&grouped, endSeconds: 5.5, words: pieces)
        let groupedResult = pass(&grouped, endSeconds: 6.0, words: pieces)
        check("sentencepiece_and_space_word_grouping", groupedResult.committed == "transcript world")

        var margin = DecodeAheadPlanner()
        let late = ("▁Late", 7.1, 7.8)
        _ = pass(&margin, endSeconds: 8.6, words: [late])
        _ = pass(&margin, endSeconds: 9.1, words: [late])
        let beforeMargin = pass(&margin, endSeconds: 9.6, words: [late])
        let afterMargin = pass(&margin, endSeconds: 10.1, words: [late])
        check("two_second_tail_margin", beforeMargin.committed.isEmpty && afterMargin.committed == "Late")

        var monotonic = DecodeAheadPlanner()
        let alpha = ("▁Alpha", 1.0, 1.4)
        let beta = ("▁Beta", 3.0, 3.4)
        let a = pass(&monotonic, endSeconds: 6.0, words: [alpha, beta])
        let b = pass(&monotonic, endSeconds: 6.5, words: [alpha, beta])
        let c = pass(&monotonic, endSeconds: 7.0, words: [alpha, beta])
        check("committed_text_monotonic", b.committed.hasPrefix(a.committed) && c.committed.hasPrefix(b.committed) && c.committed == "Alpha Beta")

        var slide = DecodeAheadPlanner()
        let middle = ("▁Middle", 10.0, 11.0)
        _ = pass(&slide, endSeconds: 12.5, words: [middle])
        _ = pass(&slide, endSeconds: 13.0, words: [middle])
        _ = pass(&slide, endSeconds: 13.5, words: [middle])
        _ = pass(&slide, endSeconds: 14.0, words: [middle])
        // No new word commits at the cap, so round 5 permits at most a 1 s slide.
        // This retains more than the required 8 s of context behind "Middle".
        check("slide_keeps_left_context", slide.committed == "Middle"
            && slide.windowStart == DecodeAheadPlanner.blankSlideStep
            && slide.windowStart <= max(0, slide.committedEnd - DecodeAheadPlanner.leftContext)
            && slide.baseOffset == 0,
            detail: "committed=\(String(reflecting: slide.committed)), windowStart=\(slide.windowStart), committedEnd=\(slide.committedEnd), baseOffset=\(slide.baseOffset)")

        var forced = DecodeAheadPlanner()
        let forcedResult = pass(&forced, endSeconds: 14.0, words: [("▁Old", 1.0, 2.0), ("▁Recent", 10.0, 11.0)])
        check("forced_commit_at_fourteen_seconds", forcedResult.committed == "Old" && forcedResult.tentative == " Recent")

        var finish = DecodeAheadPlanner()
        let finalFirst = pass(&finish, endSeconds: 1.0, words: [("▁Done", 0.1, 0.5)], final: true)
        let finalSecond = pass(&finish, endSeconds: 1.0, words: [("▁Done", 0.1, 0.5)], final: true)
        check("finalize_commits_once", finalFirst.committed == "Done" && finalSecond.committed == "Done" && !finalSecond.grewCommitted)

        var revisedFinal = DecodeAheadPlanner()
        let wrong = ("▁Wrong", 1.0, 1.4)
        _ = pass(&revisedFinal, endSeconds: 5.0, words: [wrong])
        _ = pass(&revisedFinal, endSeconds: 5.5, words: [wrong])
        _ = pass(&revisedFinal, endSeconds: 6.0, words: [wrong])
        // Final correction needs fresh audio: an already attempted window is never decoded twice.
        let corrected = pass(&revisedFinal, endSeconds: 6.1, words: [
            ("▁Right", 1.0, 1.4), ("▁ending", 2.0, 2.4)
        ], final: true)
        check("final_without_slide_uses_full_final_decode", corrected.committed == "Right ending")

        var shortFinal = DecodeAheadPlanner()
        _ = pass(&shortFinal, endSeconds: 14.0, words: [("▁Wrong", 0.5, 1.0)])
        let slidBeforeFinal = shortFinal.windowStart > 0
        shortFinal.append(Array(repeating: 0, count: Int(14.3 * Double(DecodeAheadPlanner.sampleRate)) - shortFinal.end))
        let shortFinalWindow = shortFinal.nextWindow(finalizing: true)
        var fullDecodeText = ""
        if let shortFinalWindow {
            fullDecodeText = shortFinal.apply(tokens: [
                .init(text: "▁Correct", startTime: 0.5, endTime: 1.0),
                .init(text: "▁ending", startTime: 13.8, endTime: 14.1)
            ], text: "Correct ending", modelReturnedTimings: true,
                window: shortFinalWindow, final: true).committed
        }
        check("final_short_session_equals_full_decode", slidBeforeFinal
            && shortFinal.baseOffset == 0 && shortFinalWindow?.startAbs == 0
            && shortFinalWindow?.endAbs == shortFinal.end
            && shortFinalWindow?.samples.count == shortFinal.end
            && fullDecodeText == "Correct ending",
            detail: "slid=\(slidBeforeFinal), base=\(shortFinal.baseOffset), finalWindow=\(shortFinalWindow?.startAbs ?? -1)..\(shortFinalWindow?.endAbs ?? -1), text=\(fullDecodeText)")

        var longFinal = DecodeAheadPlanner()
        longFinal.append(Array(repeating: 0, count: 20 * DecodeAheadPlanner.sampleRate))
        let longDrain = longFinal.nextWindow(finalizing: true)
        if let longDrain {
            _ = longFinal.apply(tokens: [.init(text: "▁Old", startTime: 1.0, endTime: 1.4)],
                text: "Old", modelReturnedTimings: true, window: longDrain, draining: true)
        }
        let fullTailWindow = longFinal.nextWindow(finalizing: true)
        check("final_long_session_uses_15s_window", longDrain?.endAbs == DecodeAheadPlanner.maxWindow
            && fullTailWindow?.startAbs == 5 * DecodeAheadPlanner.sampleRate
            && fullTailWindow?.endAbs == longFinal.end
            && fullTailWindow?.samples.count == DecodeAheadPlanner.maxModelSamples,
            detail: "drain=\(longDrain?.endAbs ?? -1), final=\(fullTailWindow?.startAbs ?? -1)..\(fullTailWindow?.endAbs ?? -1), samples=\(fullTailWindow?.samples.count ?? -1), base=\(longFinal.baseOffset)")

        var slidFinal = DecodeAheadPlanner()
        let earlier = ("▁Start", 1.0, 1.5)
        let previewWord = ("▁Wrong", 10.0, 10.4)
        _ = pass(&slidFinal, endSeconds: 5.0, words: [earlier])
        _ = pass(&slidFinal, endSeconds: 5.5, words: [earlier])
        _ = pass(&slidFinal, endSeconds: 6.0, words: [earlier])
        _ = pass(&slidFinal, endSeconds: 12.0, words: [earlier, previewWord])
        _ = pass(&slidFinal, endSeconds: 12.5, words: [earlier, previewWord])
        _ = pass(&slidFinal, endSeconds: 13.0, words: [earlier, previewWord])
        _ = pass(&slidFinal, endSeconds: 14.0, words: [earlier, previewWord])
        _ = pass(&slidFinal, endSeconds: 14.5, words: [earlier, previewWord])
        _ = pass(&slidFinal, endSeconds: 15.0, words: [earlier, previewWord])
        let liveWasWrong = slidFinal.committed == "Start Wrong"
        let slidPastPrefix = slidFinal.windowStart > Int(earlier.2 * Double(DecodeAheadPlanner.sampleRate))
            && slidFinal.windowStart < Int(previewWord.1 * Double(DecodeAheadPlanner.sampleRate))
        let correctedSlide = pass(&slidFinal, endSeconds: 15.1, words: [
            ("▁Right", 10.0, 10.4)
        ], final: true)
        check("final_after_slide_keeps_prior_prefix", liveWasWrong && slidPastPrefix
            && slidFinal.committed == "Start Right" && correctedSlide.committed == "Start Right")

        var blankFinal = DecodeAheadPlanner()
        let surviving = ("▁Surviving", 2.0, 2.5)
        _ = pass(&blankFinal, endSeconds: 5.0, words: [surviving])
        _ = pass(&blankFinal, endSeconds: 5.5, words: [surviving])
        _ = pass(&blankFinal, endSeconds: 6.0, words: [surviving])
        _ = pass(&blankFinal, endSeconds: 6.5, words: [surviving, ("▁candidate", 5.0, 5.4)])
        _ = pass(&blankFinal, endSeconds: 7.0, words: [], final: true)
        check("blank_final_uses_last_hypothesis", blankFinal.finalizeFromLastHypothesis() == "Surviving candidate")

        check("decode_ahead_language_hint_forwarding", DecodeAheadAsrSession.languageHint(for: "de")?.rawValue == "de"
            && DecodeAheadAsrSession.languageHint(for: nil) == nil)

        var acrossSlide = DecodeAheadPlanner()
        let start = ("▁Start", 5.0, 5.5)
        let center = ("▁Center", 9.0, 9.5)
        let ending = ("▁End", 15.0, 15.5)
        _ = pass(&acrossSlide, endSeconds: 8.0, words: [start])
        _ = pass(&acrossSlide, endSeconds: 8.5, words: [start])
        _ = pass(&acrossSlide, endSeconds: 9.0, words: [start])
        _ = pass(&acrossSlide, endSeconds: 13.0, words: [start, center])
        _ = pass(&acrossSlide, endSeconds: 13.5, words: [start, center])
        _ = pass(&acrossSlide, endSeconds: 14.0, words: [start, center])
        _ = pass(&acrossSlide, endSeconds: 18.0, words: [center, ending])
        _ = pass(&acrossSlide, endSeconds: 18.5, words: [center, ending])
        let afterSlide = pass(&acrossSlide, endSeconds: 19.0, words: [center, ending])
        check("no_duplicate_or_missing_word_across_slide", afterSlide.committed == "Start Center End")

        var fallback = DecodeAheadPlanner()
        let fallbackPreview = pass(&fallback, endSeconds: 1.0, words: [], text: "Hello", timings: false)
        let fallbackForce = pass(&fallback, endSeconds: 14.0, words: [], text: "Hello", timings: false)
        let fallbackOwnedEnd = fallback.windowStart == 14 * DecodeAheadPlanner.sampleRate
        let timinglessFullFinal = fallback.nextWindow(finalizing: true)
        let fallbackFinal = pass(&fallback, endSeconds: 15.0, words: [], text: "world", timings: false, final: true)
        // A 15 s session is fully re-decoded, so timing-less final text is authoritative.
        check("timingless_fallback", fallbackPreview.committed.isEmpty && fallbackPreview.tentative == "Hello"
            && fallbackForce.committed == "Hello" && fallbackOwnedEnd
            && timinglessFullFinal?.startAbs == 0
            && timinglessFullFinal?.endAbs == 14 * DecodeAheadPlanner.sampleRate
            && fallbackFinal.committed == "world")

        var emptyFallback = DecodeAheadPlanner()
        let emptyFallbackUpdate = pass(&emptyFallback, endSeconds: 14.0, words: [], timings: false)
        check("timingless_empty_never_claims_audio", emptyFallbackUpdate.committed.isEmpty
            && emptyFallback.committedEnd == 0
            && emptyFallback.windowStart == DecodeAheadPlanner.blankSlideStep)

        var backlog = DecodeAheadPlanner()
        let saved = ("▁Saved", 0.5, 1.5)
        _ = pass(&backlog, endSeconds: 5.0, words: [saved])
        _ = pass(&backlog, endSeconds: 5.5, words: [saved])
        _ = pass(&backlog, endSeconds: 6.0, words: [saved])
        let firstBacklogDrop = backlog.append(Array(repeating: Float(0),
            count: 121 * DecodeAheadPlanner.sampleRate - backlog.end))
        let secondBacklogDrop = backlog.append(Array(repeating: Float(0), count: DecodeAheadPlanner.sampleRate))
        check("backlog_is_bounded", firstBacklogDrop && !secondBacklogDrop && backlog.didDropPreviewAudio
            && backlog.samples.count == DecodeAheadPlanner.maxPreviewSamples
            && backlog.baseOffset == 2 * DecodeAheadPlanner.sampleRate
            && backlog.windowStart == backlog.baseOffset
            && backlog.finalizeFromLastHypothesis() == "Saved")

        var timinglessRepeat = DecodeAheadPlanner()
        _ = pass(&timinglessRepeat, endSeconds: 14.0, words: [], text: "Hello", timings: false)
        let repeatedFallback = pass(&timinglessRepeat, endSeconds: 28.0, words: [], text: "Hello", timings: false)
        let longTiminglessFinal = timinglessRepeat.nextWindow(finalizing: true)
        let repeatFinal = pass(&timinglessRepeat, endSeconds: 29.0, words: [], text: "world", timings: false, final: true)
        check("timingless_forced_commit_does_not_duplicate", repeatedFallback.committed == "Hello Hello"
            && timinglessRepeat.windowStart == 28 * DecodeAheadPlanner.sampleRate
            && longTiminglessFinal?.startAbs == 20 * DecodeAheadPlanner.sampleRate
            && repeatFinal.committed == "Hello Hello world")

        fputs("decode_ahead_v2_harness: \(failures == 0 ? "ok" : "\(failures) failure(s)")\n", stderr)
        exit(failures == 0 ? 0 : 1)
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
        if CommandLine.arguments.contains("--decode-ahead-v2-harness") {
            DecodeAheadV2Harness.run()
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
            await transcribeFile(audioPath, language: nil, translateToEnglish: false, customVocabulary: [], encoder: encoder)
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
                        let customVocabulary = decodeCustomVocabulary(from: data)
                        await transcribeFile(audioPath, language: language, translateToEnglish: translateToEnglish, customVocabulary: customVocabulary, encoder: encoder)
                    } else {
                        sendError("missing_audio_path", message: "audio_path is required", encoder: encoder)
                    }


                case "download_ctc_models":
                    await downloadCtcModels(encoder: encoder)

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
        customVocabulary: [IncomingVocabularyTerm] = [],
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
        let languageHint = language.flatMap(Language.init(rawValue:))
        if let language, languageHint == nil {
            log("⚠️ Unsupported language hint: \(language)")
        }
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
                    try await manager.transcribe(fileURL, decoderState: &decoderState, language: languageHint)
                }
                finalText = await rescoreTranscriptIfPossible(
                    result: result, audioURL: fileURL, customVocabulary: customVocabulary
                )
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

    static func downloadCtcModels(encoder: JSONEncoder) async {
        log("───────────────────────────────────────────────────────")
        log("📥 DOWNLOAD CTC MODELS REQUEST")
        log("───────────────────────────────────────────────────────")

        do {
            sendResponse(ProgressResponse(progress: 0.0, phase: "downloading ctc models"), encoder: encoder)
            try await CtcModels.download(variant: .ctc110m)

            guard ctcVocabularyReady() else {
                sendError("ctc_model_download_failed", message: "CTC model download completed but required files are missing", encoder: encoder)
                return
            }

            cachedCtcModels = nil
            cachedCtcTokenizer = nil
            cachedCtcSpotter = nil
            sendResponse(ProgressResponse(progress: 1.0, phase: "ctc models ready"), encoder: encoder)
            sendResponse(OkResponse(command: "download_ctc_models"), encoder: encoder)
        } catch {
            log("❌ CTC MODEL DOWNLOAD FAILED")
            log("❌ Error type: \(type(of: error))")
            log("❌ Error details: \(error)")
            log("❌ Localized: \(error.localizedDescription)")
            sendError("ctc_model_download_failed", message: "Failed to download CTC models: \(error.localizedDescription)", encoder: encoder)
        }

        log("───────────────────────────────────────────────────────")
    }

    static func rescoreTranscriptIfPossible(
        result: ASRResult,
        audioURL: URL,
        customVocabulary: [IncomingVocabularyTerm]
    ) async -> String {
        guard !customVocabulary.isEmpty else {
            return result.text
        }

        guard ctcVocabularyReady() else {
            log("ℹ️ Custom vocabulary skipped: CTC models not ready")
            return result.text
        }

        let directory = CtcModels.defaultCacheDirectory(for: .ctc110m)

        guard let tokenTimings = result.tokenTimings, !tokenTimings.isEmpty else {
            log("ℹ️ Custom vocabulary skipped: token timings unavailable")
            return result.text
        }

        do {
            let tokenizer = try await cachedOrLoadCtcTokenizer(from: directory)
            let terms = customVocabulary.compactMap { term -> CustomVocabularyTerm? in
                let tokenIds = tokenizer.encode(term.text)
                guard !tokenIds.isEmpty else { return nil }
                return CustomVocabularyTerm(
                    text: term.text,
                    aliases: term.aliases.isEmpty ? nil : term.aliases,
                    tokenIds: nil,
                    ctcTokenIds: tokenIds
                )
            }

            guard !terms.isEmpty else {
                log("ℹ️ Custom vocabulary skipped: no tokenizable terms")
                return result.text
            }

            let vocabulary = CustomVocabularyContext(terms: terms, minTermLength: 3)
            let models = try await cachedOrLoadCtcModels(from: directory)
            let spotter = cachedOrCreateCtcSpotter(models: models)
            let samples = try AudioConverter().resampleAudioFile(audioURL)
            let spot = try await spotter.spotKeywordsWithLogProbs(
                audioSamples: samples,
                customVocabulary: vocabulary
            )
            // Term values must never be logged. FluidAudio exposes no runtime logger level;
            // shipped sidecars are built in release so VocabularyRescorer DEBUG logs stay compiled out.
            let rescorer = try await VocabularyRescorer.create(
                spotter: spotter,
                vocabulary: vocabulary,
                ctcModelDirectory: directory
            )
            let output = rescorer.ctcTokenRescore(
                transcript: result.text,
                tokenTimings: tokenTimings,
                logProbs: spot.logProbs,
                frameDuration: spot.frameDuration
            )

            if output.wasModified {
                log("✅ Custom vocabulary applied")
                return output.text
            }

            log("ℹ️ Custom vocabulary produced no transcript changes")
            return result.text
        } catch {
            log("⚠️ Custom vocabulary rescore failed; returning original transcript. Error type: \(type(of: error))")
            return result.text
        }
    }

    static func cachedOrLoadCtcModels(from directory: URL) async throws -> CtcModels {
        if let models = cachedCtcModels {
            return models
        }

        let models = try await CtcModels.load(from: directory, variant: .ctc110m)
        cachedCtcModels = models
        return models
    }

    static func cachedOrLoadCtcTokenizer(from directory: URL) async throws -> CtcTokenizer {
        if let tokenizer = cachedCtcTokenizer {
            return tokenizer
        }

        let tokenizer = try await CtcTokenizer.load(from: directory)
        cachedCtcTokenizer = tokenizer
        return tokenizer
    }

    static func cachedOrCreateCtcSpotter(models: CtcModels) -> CtcKeywordSpotter {
        if let spotter = cachedCtcSpotter {
            return spotter
        }

        let spotter = CtcKeywordSpotter(models: models, blankId: models.vocabulary.count)
        cachedCtcSpotter = spotter
        return spotter
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
        case "load_model", "download_model", "unload_model", "delete_model", "transcribe", "download_ctc_models", "download_eou_model", "warmup", "warmup_eou", "diarize", "start_stream":
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
                let language = command["language"] as? String
                if let language, DecodeAheadAsrSession.languageHint(for: language) == nil {
                    log("⚠️ Unsupported language hint: \(language)")
                }
                let session = DecodeAheadAsrSession(
                    manager: sharedManager, decoderLayers: decoderLayers, language: language, encoder: encoder
                )
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
            await session.enqueueDecodeAheadChunk(buffer, decodeSession: decodeSession)
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

    static func decodeCustomVocabulary(from data: Data) -> [IncomingVocabularyTerm] {
        struct TranscribeCommand: Decodable {
            let custom_vocabulary: [IncomingVocabularyTerm]?
        }

        do {
            return try JSONDecoder().decode(TranscribeCommand.self, from: data).custom_vocabulary ?? []
        } catch {
            log("⚠️ Custom vocabulary ignored: command vocabulary payload could not be decoded")
            return []
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
