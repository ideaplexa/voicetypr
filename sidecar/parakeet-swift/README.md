# Parakeet Swift Sidecar

Native macOS transcription sidecar using FluidAudio SDK and Apple Neural Engine.

## Overview

This Swift sidecar replaces the previous 123MB Python/MLX implementation with a lightweight 1.2MB native binary that:

- Uses FluidAudio SDK for CoreML-based transcription
- Leverages Apple Neural Engine for hardware acceleration
- Communicates with Tauri via JSON over stdin/stdout
- Downloads models on-demand (no bundling required)

## Requirements

- macOS 14.0+ (Sonoma or later)
- Swift 6.0+
- Xcode 16 Command Line Tools

## Building

### Automated Build (via Tauri)

The sidecar is automatically built when running `pnpm tauri:dev` or `pnpm tauri build` thanks to the integration in `src-tauri/build.rs`.

### Manual Build

```bash
# Build release binary
./build.sh

# Build debug binary
./build.sh debug

# Output will be in dist/parakeet-sidecar-<target-triple>
```

## Architecture

### Communication Protocol

The sidecar communicates via JSON messages on stdin/stdout:

#### Commands (Rust → Swift)

```json
// Download and load model
{"type": "load_model", "model_id": "parakeet-tdt-0.6b-v3", "model_version": "v3", "force_download": true}

// Transcribe audio file
{"type": "transcribe", "audio_path": "/path/to/audio.wav", "language": "en"}

// Get sidecar status
{"type": "status"}

// Delete model files
{"type": "delete_model", "model_id": "parakeet-tdt-0.6b-v3", "model_version": "v3"}

// Unload model from memory
{"type": "unload_model"}

// Shutdown sidecar
{"type": "shutdown"}
```

#### Responses (Swift → Rust)

```json
// Status response
{"type": "status", "loadedModel": "parakeet-tdt-0.6b-v3", "modelVersion": "v3"}

// Transcription response
{
  "type": "transcription",
  "text": "transcribed text here",
  "segments": [],
  "language": "en",
  "duration": 5.2
}

// Error response
{
  "type": "error",
  "code": "model_not_loaded",
  "message": "Please download model first"
}
```

### Model Storage

FluidAudio manages model caching automatically:

- `~/Library/Application Support/FluidAudio/`
- `~/Library/Application Support/parakeet-tdt-0.6b-v3-coreml/`
- `~/Library/Caches/FluidAudio/`

Models are downloaded once (~500MB) and reused across app launches.

## Integration with Tauri

### Configuration

`src-tauri/tauri.conf.json`:
```json
{
  "bundle": {
    "externalBin": [
      "../sidecar/parakeet-swift/dist/parakeet-sidecar"
    ]
  }
}
```

### Capabilities

`src-tauri/capabilities/macos.json`:
```json
{
  "permissions": [
    {
      "identifier": "shell:allow-spawn",
      "allow": [
        { "name": "parakeet-sidecar", "sidecar": true },
        { "name": "parakeet-sidecar-aarch64-apple-darwin", "sidecar": true }
      ]
    },
    {
      "identifier": "shell:allow-stdin-write",
      "allow": [
        { "name": "parakeet-sidecar", "sidecar": true },
        { "name": "parakeet-sidecar-aarch64-apple-darwin", "sidecar": true }
      ]
    }
  ]
}
```

### Rust Integration

The sidecar is spawned via `ParakeetClient` in `src-tauri/src/parakeet/sidecar.rs`:

```rust
let client = ParakeetClient::new("parakeet-sidecar");
let response = client.send(&app, &ParakeetCommand::Status {}).await?;
```

## Development

### Testing the Sidecar

```bash
# Build the sidecar
./build.sh

# Test status command
echo '{"type":"status"}' | ./dist/parakeet-sidecar-aarch64-apple-darwin

# Expected output:
# {"type":"status","loaded_model":null,"model_path":null,"precision":null,"attention":null}
```

### Debugging

Enable debug logging by running the sidecar with commands directly:

```bash
# Test transcription
./dist/parakeet-sidecar-aarch64-apple-darwin /path/to/audio.wav
```

## Troubleshooting

### Build Errors

**Error**: `error: unable to find utility "xcrun"`
**Solution**: Install Xcode Command Line Tools:
```bash
xcode-select --install
```

**Error**: `No such module 'FluidAudio'`
**Solution**: Clean and rebuild:
```bash
rm -rf .build Package.resolved
swift build -c release
```

### Runtime Errors

**Error**: `model_not_loaded`
**Solution**: Download the model first via the UI Settings → Download button

**Error**: `file_not_found`
**Solution**: Ensure audio file exists and is in a supported format (WAV, MP3, M4A, etc.)

## Performance

- Binary size: **1.2MB** (vs 123MB Python version)
- Memory usage: ~100MB during transcription
- Transcription speed: Real-time on Apple Silicon with ANE
- Model download: One-time ~500MB (managed by FluidAudio)

## License

Same as Voicetypr parent project.
