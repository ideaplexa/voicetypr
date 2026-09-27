#!/bin/bash

# Release script for Voicetypr with Separate Architecture Binaries and Built-in Tauri Notarization
# Usage: 
#   ./scripts/release-separate.sh [patch|minor|major]            - Full release
#   ./scripts/release-separate.sh [patch|minor|major] --dry-run  - Preview what would happen
#   ./scripts/release-separate.sh --build-only                   - Build & upload only (skip version bump)

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color
GITHUB_REPOSITORY="ideaplexa/voicetypr"

# Parse arguments
BUILD_ONLY=false
DRY_RUN=false
RELEASE_TYPE=""

# Parse all arguments
for arg in "$@"; do
    case "$arg" in
        --build-only)
            BUILD_ONLY=true
            ;;
        --dry-run)
            DRY_RUN=true
            ;;
        patch|minor|major)
            RELEASE_TYPE="$arg"
            ;;
        *)
            echo -e "${RED}Usage: $0 [patch|minor|major|--build-only] [--dry-run]${NC}"
            exit 1
            ;;
    esac
done

# Validate arguments
if [[ "$BUILD_ONLY" == false && -z "$RELEASE_TYPE" ]]; then
    echo -e "${RED}Usage: $0 [patch|minor|major|--build-only] [--dry-run]${NC}"
    exit 1
fi

if [[ "$DRY_RUN" == true ]]; then
    echo -e "${BLUE}=== DRY RUN MODE - No changes will be made ===${NC}"
fi

# Trap to ensure cleanup happens even on error
trap 'echo -e "${RED}Script failed! Check the error above.${NC}"' ERR

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}Error: Required command not found: $cmd${NC}"
        exit 1
    fi
}

require_file() {
    local path="$1"
    if [[ ! -f "$path" ]]; then
        echo -e "${RED}Error: Required file not found: $path${NC}"
        exit 1
    fi
}

# Match the minimum Node version used by release CI before reading credentials
# or starting builds (the frontend toolchain also needs a modern Node runtime).
require_cmd node
if ! node -e 'const [major, minor] = process.versions.node.split(".").map(Number); process.exit(major > 22 || (major === 22 && minor >= 19) ? 0 : 1)'; then
    echo -e "${RED}Error: Release builds require Node.js 22.19.0 or newer.${NC}"
    exit 1
fi

# Load .env file if it exists
if [ -f .env ]; then
    echo -e "${YELLOW}Loading environment variables from .env...${NC}"
    set -a
    source .env
    set +a
    echo -e "${GREEN}✓ Environment variables loaded${NC}"
fi

# Check for required environment variables
echo -e "${YELLOW}Checking environment variables...${NC}"

# Check Apple signing/notarization credentials
MISSING_VARS=()
if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    MISSING_VARS+=("APPLE_SIGNING_IDENTITY")
fi

# Check for notarization credentials (API key method preferred)
if [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
    echo -e "${GREEN}✓ Using API key authentication${NC}"
    if [[ -z "${APPLE_API_KEY_PATH:-}" ]]; then
        # Try common locations for AuthKey_XXXX.p8
        CANDIDATES=(
            "$HOME/.private_keys/AuthKey_${APPLE_API_KEY}.p8"
            "$HOME/private_keys/AuthKey_${APPLE_API_KEY}.p8"
            "$HOME/Downloads/AuthKey_${APPLE_API_KEY}.p8"
            "$PWD/AuthKey_${APPLE_API_KEY}.p8"
        )
        for candidate in "${CANDIDATES[@]}"; do
            if [[ -f "$candidate" ]]; then
                export APPLE_API_KEY_PATH="$candidate"
                echo -e "${GREEN}✓ Found APPLE_API_KEY_PATH at $APPLE_API_KEY_PATH${NC}"
                break
            fi
        done
    fi

    if [[ -z "${APPLE_API_KEY_PATH:-}" ]]; then
        echo -e "${RED}Error: APPLE_API_KEY_PATH not set and AuthKey file not found${NC}"
        MISSING_VARS+=("APPLE_API_KEY_PATH")
    fi
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    echo -e "${GREEN}✓ Using Apple ID authentication${NC}"
else
    echo -e "${RED}Error: Missing notarization credentials${NC}"
    echo "Set either:"
    echo "  1. API Key method (recommended):"
    echo "     export APPLE_API_KEY='your-api-key'"
    echo "     export APPLE_API_ISSUER='your-issuer-id'"
    echo "     export APPLE_API_KEY_PATH='/path/to/AuthKey_XXXXX.p8'"
    echo "  2. Apple ID method:"
    echo "     export APPLE_ID='your@email.com'"
    echo "     export APPLE_PASSWORD='xxxx-xxxx-xxxx-xxxx'"
    echo "     export APPLE_TEAM_ID='XXXXXXXXXX'"
    MISSING_VARS+=("notarization credentials")
fi

# Check for Tauri signing credentials - also check common path
TAURI_KEY_PATH="$HOME/.tauri/voicetypr.key"
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] && [[ -z "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]] && [[ ! -f "$TAURI_KEY_PATH" ]]; then
    echo -e "${RED}Error: Tauri signing key not configured${NC}"
    echo "Update signatures are required for auto-updates. Configure one of:"
    echo "1. Generate keys: cargo tauri signer generate -w ~/.tauri/voicetypr.key"
    echo "2. Set one of:"
    echo "   export TAURI_SIGNING_PRIVATE_KEY_PATH=\"$HOME/.tauri/voicetypr.key\""
    echo "   export TAURI_SIGNING_PRIVATE_KEY=\"\$(cat ~/.tauri/voicetypr.key)\""
    echo "3. If key has password: export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=\"your-password\""
    MISSING_VARS+=("TAURI signing key")
elif [[ -f "$TAURI_KEY_PATH" ]] && [[ -z "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
    # Auto-detect key at common location
    export TAURI_SIGNING_PRIVATE_KEY_PATH="$TAURI_KEY_PATH"
    echo -e "${GREEN}✓ Tauri signing key found at $TAURI_KEY_PATH${NC}"
else
    echo -e "${GREEN}✓ Tauri signing configured${NC}"
fi

if [[ ${#MISSING_VARS[@]} -gt 0 ]]; then
    echo -e "${RED}Error: Missing required environment variables: ${MISSING_VARS[*]}${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Environment variables configured${NC}"
echo -e "  Signing Identity: ${APPLE_SIGNING_IDENTITY}"
if [[ -n "${APPLE_TEAM_ID:-}" ]]; then
    echo -e "  Team ID: ${APPLE_TEAM_ID}"
fi

# Set CI mode for non-interactive operation
export CI=true

if [[ "$BUILD_ONLY" == true ]]; then
    echo -e "${GREEN}🔨 Starting Voicetypr BUILD-ONLY mode${NC}"
else
    echo -e "${GREEN}🚀 Starting Voicetypr release process (${RELEASE_TYPE})${NC}"
fi

require_cmd git
require_cmd pnpm
require_cmd jq
require_cmd cargo
require_cmd gh
require_file package.json
require_file src-tauri/Cargo.toml

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" != "main" ]]; then
    echo -e "${RED}Error: Must run releases from main branch (currently on ${CURRENT_BRANCH})${NC}"
    exit 1
fi

# Check for uncommitted changes
if [[ -n $(git status -s) ]]; then
    if [[ "$DRY_RUN" == true ]]; then
        echo -e "${YELLOW}Warning: You have uncommitted changes (ignored for dry-run)${NC}"
        git status -s
    else
        echo -e "${RED}Error: You have uncommitted changes${NC}"
        git status -s
        exit 1
    fi
fi

# Pull latest changes (skip for dry-run)
if [[ "$DRY_RUN" == true ]]; then
    echo -e "${BLUE}[DRY RUN] Would pull latest changes${NC}"
else
    echo -e "${YELLOW}Pulling latest changes...${NC}"
    git pull --ff-only origin main
fi

if [[ "$BUILD_ONLY" == true ]]; then
    # BUILD-ONLY MODE: Get version and verify tag/release exist
    NEW_VERSION=$(node -p "require('./package.json').version")
    echo -e "${GREEN}Using existing version: ${NEW_VERSION}${NC}"
    
    # Verify tag exists
    if ! git tag -l "v${NEW_VERSION}" | grep -q "v${NEW_VERSION}"; then
        echo -e "${RED}Error: Tag v${NEW_VERSION} does not exist${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Tag v${NEW_VERSION} exists${NC}"
    
    # Verify draft release exists
    if ! gh release view "v${NEW_VERSION}" --repo "$GITHUB_REPOSITORY" &>/dev/null; then
        echo -e "${RED}Error: GitHub release v${NEW_VERSION} does not exist${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Draft release v${NEW_VERSION} exists${NC}"
else
    # FULL RELEASE MODE: prepare local version for build; publish only after successful build
    
    # Run typecheck first (was in release-it before:init)
    echo -e "${YELLOW}Running typecheck...${NC}"
    pnpm typecheck

    # Run backend tests
    echo -e "${YELLOW}Running backend tests...${NC}"
    pnpm test:backend

    # Check there are commits since last tag (was requireCommits in release-it)
    LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
    if [[ -n "$LAST_TAG" ]]; then
        COMMIT_COUNT=$(git rev-list "${LAST_TAG}..HEAD" --count)
        if [[ "$COMMIT_COUNT" -eq 0 ]]; then
            echo -e "${RED}Error: No commits since last tag ${LAST_TAG}${NC}"
            exit 1
        fi
        echo -e "${GREEN}✓ Found ${COMMIT_COUNT} commits since ${LAST_TAG}${NC}"
    fi

    # Get current version
    CURRENT_VERSION=$(node -p "require('./package.json').version")
    echo -e "${GREEN}Current version: ${CURRENT_VERSION}${NC}"

    # Calculate new version
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
    case "$RELEASE_TYPE" in
        major) NEW_VERSION="$((MAJOR + 1)).0.0" ;;
        minor) NEW_VERSION="${MAJOR}.$((MINOR + 1)).0" ;;
        patch) NEW_VERSION="${MAJOR}.${MINOR}.$((PATCH + 1))" ;;
    esac
    echo -e "${GREEN}New version: ${NEW_VERSION}${NC}"

    # Check tag doesn't already exist before running long build steps
    if git ls-remote --tags origin | grep -q "refs/tags/v${NEW_VERSION}"; then
        echo -e "${RED}Error: Tag v${NEW_VERSION} already exists on origin${NC}"
        exit 1
    fi

    if [[ "$DRY_RUN" == true ]]; then
        echo -e "${BLUE}[DRY RUN] Would bump version: ${CURRENT_VERSION} → ${NEW_VERSION}${NC}"
        echo -e "${BLUE}[DRY RUN] Would update working tree for build: package.json, Cargo.toml${NC}"
        echo -e "${BLUE}[DRY RUN] Would build aarch64-apple-darwin (notarized)${NC}"
        echo -e "${BLUE}[DRY RUN] Would build x86_64-apple-darwin (notarized)${NC}"
        echo -e "${BLUE}[DRY RUN] Would sign artifacts and create latest.json${NC}"
        echo -e "${BLUE}[DRY RUN] Would then generate changelog, commit, tag, push, create draft release, and upload artifacts${NC}"
        echo ""
        echo -e "${GREEN}=== DRY RUN COMPLETE - No changes made ===${NC}"
        exit 0
    fi

    # Bump version
    echo -e "${YELLOW}Bumping version (${RELEASE_TYPE})...${NC}"
    npm version "$RELEASE_TYPE" --no-git-tag-version

    # Update Cargo.toml
    echo -e "${YELLOW}Updating Cargo.toml...${NC}"
    sed -i '' "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" src-tauri/Cargo.toml
fi

# Install required Rust targets if not already installed
echo -e "${YELLOW}Checking Rust targets...${NC}"
rustup target add aarch64-apple-darwin 2>/dev/null || true
rustup target add x86_64-apple-darwin 2>/dev/null || true

# Create output directory
OUTPUT_DIR="release-${NEW_VERSION}"
mkdir -p "$OUTPUT_DIR"

# Function to sign update artifacts
sign_update_artifact() {
    local FILE_PATH="$1"
    
    if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] || [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
        echo -e "${YELLOW}Signing $(basename "$FILE_PATH")...${NC}"
        
        # Determine if we have a key path or key content
        if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
            KEY_PATH="$TAURI_SIGNING_PRIVATE_KEY_PATH"
        else
            # It's key content - write to temp file
            TEMP_KEY=$(mktemp)
            echo "${TAURI_SIGNING_PRIVATE_KEY}" > "$TEMP_KEY"
            KEY_PATH="$TEMP_KEY"
        fi
        
        # Sign with proper flags (use pnpm tauri, not cargo tauri)
        pnpm tauri signer sign -f "$KEY_PATH" -p "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD-}" "$FILE_PATH"
        
        # Clean up temp file if created
        if [[ -n "${TEMP_KEY:-}" ]] && [[ -f "$TEMP_KEY" ]]; then
            rm -f "$TEMP_KEY"
        fi
        
        if [[ -f "${FILE_PATH}.sig" ]]; then
            echo -e "${GREEN}✓ Signature created for $(basename "$FILE_PATH")${NC}"
            return 0
        else
            echo -e "${RED}Warning: Signature file not created for $(basename "$FILE_PATH")${NC}"
            return 1
        fi
    else
        echo -e "${RED}Error: Missing Tauri signing key; cannot sign update artifacts${NC}"
        return 1
    fi
}

# Build Parakeet sidecar (Apple Silicon, arm64) and prepare dist symlink
build_parakeet_sidecar() {
    echo -e "${YELLOW}Building Parakeet sidecar (arm64)...${NC}"
    local SIDE_DIR="sidecar/parakeet-swift"
    if [[ ! -d "$SIDE_DIR" ]]; then
        echo -e "${YELLOW}Parakeet sidecar directory not found at $SIDE_DIR; skipping sidecar build${NC}"
        return 0
    fi

    if ! command -v swift >/dev/null 2>&1; then
        echo -e "${RED}Swift toolchain not found. Install Xcode Command Line Tools to build sidecar.${NC}"
        exit 1
    fi

    pushd "$SIDE_DIR" > /dev/null

    # Build for Apple Silicon
    swift build -c release --arch arm64
    # Determine binary output path
    BIN_DIR=$(swift build -c release --arch arm64 --show-bin-path 2>/dev/null || echo ".build/arm64-apple-macosx/release")
    SRC_BIN_NAME="ParakeetSidecar"
    SRC_BIN_PATH="$BIN_DIR/$SRC_BIN_NAME"

    if [[ ! -f "$SRC_BIN_PATH" ]]; then
        echo -e "${RED}Error: Built sidecar not found at $SRC_BIN_PATH${NC}"
        popd > /dev/null
        exit 1
    fi

    mkdir -p dist
    cp "$SRC_BIN_PATH" "dist/parakeet-sidecar-aarch64-apple-darwin"
    chmod +x "dist/parakeet-sidecar-aarch64-apple-darwin"
    ln -sfn "parakeet-sidecar-aarch64-apple-darwin" "dist/parakeet-sidecar"

    echo -e "${GREEN}✓ Parakeet sidecar built and prepared at $SIDE_DIR/dist${NC}"
    popd > /dev/null
}

## Ensure sidecar is built and available for bundling
# Note: Parakeet sidecar is Apple Silicon only (FluidAudio requires Apple Neural Engine)
# Intel Mac users will only have Whisper available (CPU-only mode)
build_parakeet_sidecar

# Build aarch64 binary with automatic notarization
echo -e "${GREEN}🔨 Building aarch64 binary with notarization...${NC}"
echo -e "${BLUE}This will take some time as it includes notarization...${NC}"

# Build with an override config for signing identity (version comes from Cargo.toml)
TAURI_CONFIG_OVERRIDE=$(jq -nc --arg identity "$APPLE_SIGNING_IDENTITY" '{bundle:{macOS:{signingIdentity:$identity}}}')
pnpm -s tauri build --target aarch64-apple-darwin --bundles app,dmg --config "$TAURI_CONFIG_OVERRIDE" --ci

# Find aarch64 build artifacts
AARCH64_DMG=$(find "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
AARCH64_APP_DIR="src-tauri/target/aarch64-apple-darwin/release/bundle/macos"

# Create app.tar.gz for aarch64
echo -e "${YELLOW}Creating aarch64 updater archive...${NC}"
APP_BUNDLE_PATH=$(find "$AARCH64_APP_DIR" -maxdepth 1 -name "*.app" | head -n 1)
if [[ -n "${APP_BUNDLE_PATH}" && -d "$APP_BUNDLE_PATH" ]]; then
    cd "$AARCH64_APP_DIR"
    APP_BUNDLE_NAME=$(basename "$APP_BUNDLE_PATH")
    COPYFILE_DISABLE=1 tar -czf "Voicetypr_${NEW_VERSION}_aarch64.app.tar.gz" --exclude='._*' --exclude='.DS_Store' "$APP_BUNDLE_NAME"
    cd - > /dev/null
    AARCH64_APP_TAR="$AARCH64_APP_DIR/Voicetypr_${NEW_VERSION}_aarch64.app.tar.gz"
else
    echo -e "${RED}Error: aarch64 app bundle not found${NC}"
    exit 1
fi

if [[ -z "$AARCH64_DMG" ]]; then
    echo -e "${RED}Error: aarch64 DMG not found${NC}"
    exit 1
fi

# Copy aarch64 artifacts
cp "$AARCH64_DMG" "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_aarch64.dmg"
cp "$AARCH64_APP_TAR" "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_aarch64.app.tar.gz"

# Sign aarch64 update artifact
sign_update_artifact "$AARCH64_APP_TAR" || {
    echo -e "${RED}Error: Failed to sign update artifact${NC}"
    exit 1
}
if [[ -f "${AARCH64_APP_TAR}.sig" ]]; then
    cp "${AARCH64_APP_TAR}.sig" "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_aarch64.app.tar.gz.sig"
fi

# Build x86_64 (Intel Mac) binary with automatic notarization
# Note: Intel Mac only gets Whisper (CPU mode) - Parakeet requires Apple Silicon
echo -e "${GREEN}🔨 Building x86_64 (Intel Mac) binary with notarization...${NC}"
echo -e "${BLUE}This will take some time as it includes notarization...${NC}"

pnpm -s tauri build --target x86_64-apple-darwin --bundles app,dmg --config "$TAURI_CONFIG_OVERRIDE" --ci

# Find x86_64 build artifacts
X86_64_DMG=$(find "src-tauri/target/x86_64-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -n 1)
X86_64_APP_DIR="src-tauri/target/x86_64-apple-darwin/release/bundle/macos"

# Create app.tar.gz for x86_64
echo -e "${YELLOW}Creating x86_64 updater archive...${NC}"
X86_64_APP_BUNDLE_PATH=$(find "$X86_64_APP_DIR" -maxdepth 1 -name "*.app" | head -n 1)
if [[ -n "${X86_64_APP_BUNDLE_PATH}" && -d "$X86_64_APP_BUNDLE_PATH" ]]; then
    cd "$X86_64_APP_DIR"
    X86_64_APP_BUNDLE_NAME=$(basename "$X86_64_APP_BUNDLE_PATH")
    COPYFILE_DISABLE=1 tar -czf "Voicetypr_${NEW_VERSION}_x86_64.app.tar.gz" --exclude='._*' --exclude='.DS_Store' "$X86_64_APP_BUNDLE_NAME"
    cd - > /dev/null
    X86_64_APP_TAR="$X86_64_APP_DIR/Voicetypr_${NEW_VERSION}_x86_64.app.tar.gz"
else
    echo -e "${RED}Error: x86_64 app bundle not found${NC}"
    exit 1
fi

if [[ -z "$X86_64_DMG" ]]; then
    echo -e "${RED}Error: x86_64 DMG not found${NC}"
    exit 1
fi

# Copy x86_64 artifacts
cp "$X86_64_DMG" "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_x86_64.dmg"
cp "$X86_64_APP_TAR" "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_x86_64.app.tar.gz"

# Sign x86_64 update artifact
sign_update_artifact "$X86_64_APP_TAR" || {
    echo -e "${RED}Error: Failed to sign x86_64 update artifact${NC}"
    exit 1
}
if [[ -f "${X86_64_APP_TAR}.sig" ]]; then
    cp "${X86_64_APP_TAR}.sig" "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_x86_64.app.tar.gz.sig"
fi

# Create latest.json for updater (both architectures)
echo -e "${YELLOW}Creating latest.json...${NC}"

# Get aarch64 signature from the sig file if it exists
if [[ -f "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_aarch64.app.tar.gz.sig" ]]; then
    AARCH64_SIGNATURE=$(cat "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_aarch64.app.tar.gz.sig" | tr -d '\n')
else
    echo -e "${RED}Error: No aarch64 signature file found${NC}"
    exit 1
fi

# Get x86_64 signature from the sig file if it exists
if [[ -f "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_x86_64.app.tar.gz.sig" ]]; then
    X86_64_SIGNATURE=$(cat "$OUTPUT_DIR/Voicetypr_${NEW_VERSION}_x86_64.app.tar.gz.sig" | tr -d '\n')
else
    echo -e "${RED}Error: No x86_64 signature file found${NC}"
    exit 1
fi

# Create latest.json with both architectures (Apple Silicon + Intel Mac)
printf '{
  "version": "v%s",
  "notes": "See the release notes for v%s",
  "pub_date": "%s",
  "platforms": {
    "darwin-aarch64": {
      "signature": "%s",
      "url": "https://github.com/%s/releases/download/v%s/Voicetypr_%s_aarch64.app.tar.gz"
    },
    "darwin-x86_64": {
      "signature": "%s",
      "url": "https://github.com/%s/releases/download/v%s/Voicetypr_%s_x86_64.app.tar.gz"
    }
  }
}\n' "$NEW_VERSION" "$NEW_VERSION" "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$AARCH64_SIGNATURE" "$GITHUB_REPOSITORY" "$NEW_VERSION" "$NEW_VERSION" "$X86_64_SIGNATURE" "$GITHUB_REPOSITORY" "$NEW_VERSION" "$NEW_VERSION" > "$OUTPUT_DIR/latest.json"

# Verify notarization
echo -e "${BLUE}✅ Verifying notarization...${NC}"

# Check aarch64 app bundle
if [[ -n "${APP_BUNDLE_PATH}" && -d "$APP_BUNDLE_PATH" ]]; then
    spctl -a -t exec -vv "$APP_BUNDLE_PATH" 2>&1 | grep -q "accepted" && {
        echo -e "${GREEN}✓ aarch64 app bundle is properly notarized${NC}"
    } || {
        echo -e "${YELLOW}Warning: aarch64 app bundle notarization check failed${NC}"
    }
fi

# Check x86_64 app bundle
if [[ -n "${X86_64_APP_BUNDLE_PATH}" && -d "$X86_64_APP_BUNDLE_PATH" ]]; then
    spctl -a -t exec -vv "$X86_64_APP_BUNDLE_PATH" 2>&1 | grep -q "accepted" && {
        echo -e "${GREEN}✓ x86_64 app bundle is properly notarized${NC}"
    } || {
        echo -e "${YELLOW}Warning: x86_64 app bundle notarization check failed${NC}"
    }
fi

if [[ "$BUILD_ONLY" == false ]]; then
    # Re-check remote state before publishing version/tag
    git fetch origin main --tags
    if [[ "$(git rev-parse origin/main)" != "$(git rev-parse HEAD)" ]]; then
        echo -e "${RED}Error: origin/main advanced during build. Re-run release from latest main.${NC}"
        exit 1
    fi
    if git ls-remote --tags origin | grep -q "refs/tags/v${NEW_VERSION}"; then
        echo -e "${RED}Error: Tag v${NEW_VERSION} was created while build was running. Re-run release.${NC}"
        exit 1
    fi

    # Generate changelog only after successful build/sign verification.
    # Resolves the pinned local conventional-changelog-cli (no network fetch); -r 1 -u is
    # required: without them the pre-tag run silently emits an empty changelog.
    pnpm exec conventional-changelog -p angular -i CHANGELOG.md -s -r 1 -u

    # Commit, tag, push
    echo -e "${YELLOW}Committing and tagging...${NC}"
    git add package.json src-tauri/Cargo.toml CHANGELOG.md
    git commit -m "chore: release v${NEW_VERSION}"
    git tag "v${NEW_VERSION}"
    git push origin main
    git push origin "v${NEW_VERSION}"

    # Create draft GitHub release
    echo -e "${YELLOW}Creating draft GitHub release...${NC}"
    gh release create "v${NEW_VERSION}" --repo "$GITHUB_REPOSITORY" --draft --title "Voicetypr v${NEW_VERSION}" --generate-notes
    echo -e "${GREEN}✓ Draft release v${NEW_VERSION} created${NC}"
fi

# Upload artifacts to the release
echo -e "${YELLOW}Uploading artifacts to GitHub release v${NEW_VERSION}...${NC}"
gh release view "v${NEW_VERSION}" --repo "$GITHUB_REPOSITORY" >/dev/null
for file in "$OUTPUT_DIR"/*; do
    echo -e "  Uploading: $(basename "$file")"
    gh release upload "v${NEW_VERSION}" "$file" --repo "$GITHUB_REPOSITORY" --clobber
done
echo -e "${GREEN}✓ All artifacts uploaded successfully${NC}"

echo -e "${GREEN}✅ Release process complete!${NC}"
echo -e "${GREEN}📁 Notarized artifacts saved in: ${OUTPUT_DIR}/${NC}"
echo ""
echo -e "${BLUE}📦 Release artifacts:${NC}"
ls -lh "$OUTPUT_DIR" | grep -E '\.(dmg|tar\.gz|sig|json)$' | while read -r line; do
    echo "   $line"
done
echo ""
echo -e "${YELLOW}📋 Next steps:${NC}"
echo "1. Review the draft release on GitHub"
echo "2. Test the notarized DMG (Apple Silicon aarch64)"
echo "3. Test the notarized DMG (Intel Mac x86_64)"
echo "4. Verify auto-updater works with the new signatures"
echo "5. Publish the release when ready"
echo ""
echo -e "${YELLOW}📝 Intel Mac Notes:${NC}"
echo "   - Parakeet models are NOT available (requires Apple Neural Engine)"
echo "   - Intel Mac users can only use Whisper (CPU-only mode)"
echo ""
echo -e "${GREEN}🔗 Release URL: https://github.com/${GITHUB_REPOSITORY}/releases/tag/v${NEW_VERSION}${NC}"
echo -e "${GREEN}🎉 Both Apple Silicon and Intel Mac apps are now fully notarized and ready for distribution!${NC}"
