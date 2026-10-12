#!/usr/bin/env node
// Ensures ffmpeg/ffprobe sidecar binaries exist without relying on system installs.
// Downloads pinned archives per platform, extracts, and places binaries under sidecar/ffmpeg/dist.
// macOS (arm64): sidecar/ffmpeg/dist/ffmpeg, ffprobe (+ arch symlinks)
// Windows (x64): sidecar/ffmpeg/dist/ffmpeg.exe, ffprobe.exe (+ target-triple copies)
const fs = require('fs');
const path = require('path');
const os = require('os');
const cp = require('child_process');
const crypto = require('crypto');

function ensureDir(p) {
  if (!fs.existsSync(p)) fs.mkdirSync(p, { recursive: true });
}

function run(cmd, args, cwd) {
  cp.execFileSync(cmd, args, { stdio: 'inherit', cwd });
}

function download(url, outFile) {
  console.log(`[ensure-ffmpeg-sidecar] Downloading ${url}`);
  // Use curl if available
  try {
    // -sS: silent but show errors; -L: follow redirects; -f: fail on HTTP errors
    // --http1.1: force HTTP/1.1 to avoid HTTP/2 protocol errors on some servers
    run('curl', ['--http1.1', '-sS', '-L', '-f', '-o', outFile, url]);
    return;
  } catch (e) {
    // Fallback to powershell on Windows
    if (process.platform === 'win32') {
      cp.execSync(
        `powershell -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing -Uri '${url}' -OutFile '${outFile.replace(/'/g, "''")}'"`,
        { stdio: 'inherit' }
      );
      return;
    }
    throw e;
  }
}

function unzip(zipFile, outDir) {
  ensureDir(outDir);
  if (process.platform === 'win32') {
    // Use PowerShell Expand-Archive
    cp.execSync(
      `powershell -Command "Expand-Archive -Path '${zipFile.replace(/'/g, "''")}' -DestinationPath '${outDir.replace(/'/g, "''")}' -Force"`,
      { stdio: 'inherit' }
    );
  } else {
    // macOS has unzip by default
    run('unzip', ['-o', zipFile, '-d', outDir]);
  }
}

function chmodx(p) {
  try { if (process.platform !== 'win32') fs.chmodSync(p, 0o755); } catch (_) {}
}

function ensureSymlink(target, link) {
  try {
    if (fs.existsSync(link)) fs.unlinkSync(link);
    fs.symlinkSync(path.basename(target), link);
  } catch (err) {
    console.warn(`[ensure-ffmpeg-sidecar] Failed to create symlink ${link}: ${err}`);
  }
}

function ensureCopy(src, dst) {
  try { if (!fs.existsSync(dst)) fs.copyFileSync(src, dst); } catch (err) {
    console.warn(`[ensure-ffmpeg-sidecar] Failed to create copy ${dst}: ${err}`);
  }
}

function fail(msg) {
  console.error(`\n[ensure-ffmpeg-sidecar] ${msg}\n`);
  process.exit(1);
}

function cleanupTmp(tmpDir) {
  try {
    if (tmpDir && fs.existsSync(tmpDir)) {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  } catch (err) {
    console.warn(`[ensure-ffmpeg-sidecar] Failed to cleanup temp dir ${tmpDir}: ${err.message}`);
  }
}

function sha256File(file) {
  const buf = fs.readFileSync(file);
  return crypto.createHash('sha256').update(buf).digest('hex');
}

function verifyChecksum(file, expected, label) {
  if (!expected) {
    console.log(`[ensure-ffmpeg-sidecar] Skipping checksum for ${label} (no expected hash provided).`);
    return;
  }
  const actual = sha256File(file);
  if (actual.toLowerCase() !== expected.toLowerCase()) {
    fail(`${label} checksum mismatch. Expected ${expected}, got ${actual}`);
  }
  console.log(`[ensure-ffmpeg-sidecar] Verified ${label} SHA256: ${actual}`);
}

const TAURI_CONFIGS = [
  'src-tauri/tauri.conf.json',
  'src-tauri/tauri.windows.conf.json',
];

const OBSOLETE_WINDOWS_DOUBLE_EXTENSION_COPIES = [
  'ffmpeg.exe-x86_64-pc-windows-msvc.exe',
  'ffprobe.exe-x86_64-pc-windows-msvc.exe',
];

function assertExtensionlessExternalBins() {
  const offenders = [];

  for (const config of TAURI_CONFIGS) {
    const configPath = path.resolve(__dirname, '..', config);
    if (!fs.existsSync(configPath)) continue;

    let parsed;
    try {
      parsed = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    } catch (err) {
      fail(`Failed to parse ${config}: ${err.message}`);
    }

    for (const entry of parsed.bundle?.externalBin ?? []) {
      if (path.win32.basename(entry).toLowerCase().endsWith('.exe')) {
        offenders.push(`${config}: ${entry}`);
      }
    }
  }

  if (offenders.length > 0) {
    fail(
      'Tauri externalBin entries must be extensionless. ' +
        'Tauri appends the target triple and platform extension itself; .exe entries package as .exe.exe on Windows.\n' +
        offenders.join('\n')
    );
  }
}

function removeObsoleteDoubleExtensionCopies(distDir) {
  for (const file of OBSOLETE_WINDOWS_DOUBLE_EXTENSION_COPIES) {
    const obsoletePath = path.join(distDir, file);
    if (fs.existsSync(obsoletePath)) {
      fs.rmSync(obsoletePath, { force: true });
      console.log(`[ensure-ffmpeg-sidecar] Removed obsolete double-extension sidecar copy: ${file}`);
    }
  }
}


(function main() {
  const distDir = path.resolve(__dirname, '..', 'sidecar', 'ffmpeg', 'dist');
  ensureDir(distDir);
  assertExtensionlessExternalBins();
  removeObsoleteDoubleExtensionCopies(distDir);

  const isMac = process.platform === 'darwin';
  const isWin = process.platform === 'win32';

  if (isMac) {
    if (process.arch !== 'arm64') {
      console.log('[ensure-ffmpeg-sidecar] Intel Mac detected - will download binaries for both architectures.');
    }
    const ffmpegDst = path.join(distDir, 'ffmpeg');
    const ffprobeDst = path.join(distDir, 'ffprobe');
    const ffmpegX64Dst = path.join(distDir, 'ffmpeg-x86_64-apple-darwin');
    const ffprobeX64Dst = path.join(distDir, 'ffprobe-x86_64-apple-darwin');
    const macFfmpegBinSha = process.env.FFMPEG_MAC_BIN_SHA256 || '77d2c853f431318d55ec02676d9b2f185ebfdddb9f7677a251fbe453affe025a';
    const macFfprobeBinSha = process.env.FFPROBE_MAC_BIN_SHA256 || 'babf170e86bd6b0b2fefee5fa56f57721b0acb98ad2794b095d8030b02857dfe';
    // Pinned macOS Intel 9.0.1 from evermeet.cx (override via env if needed)
    const ffmpegX64Url = process.env.FFMPEG_MAC_X64_URL || 'https://evermeet.cx/ffmpeg/ffmpeg-9.0.1.zip';
    const ffprobeX64Url = process.env.FFPROBE_MAC_X64_URL || 'https://evermeet.cx/ffmpeg/ffprobe-9.0.1.zip';
    const macX64FfmpegBinSha = process.env.FFMPEG_MAC_X64_BIN_SHA256 || 'e27de05e3a9f9c758f9766d15d1a069fddeed5f725e35d9ab28683be4740dad7';
    const macX64FfprobeBinSha = process.env.FFPROBE_MAC_X64_BIN_SHA256 || 'a1508faa028bfb8e20c9d182c3d41fcff29ee7584ae21b2d6c357472a3ecbc24';

    // Check if ARM64 binaries exist
    const arm64Present = fs.existsSync(ffmpegDst) && fs.existsSync(ffprobeDst);
    if (arm64Present) {
      // Revalidate restored/cached binaries before allowing the early-return paths below.
      verifyChecksum(ffmpegDst, macFfmpegBinSha, 'macOS ffmpeg (binary)');
      verifyChecksum(ffprobeDst, macFfprobeBinSha, 'macOS ffprobe (binary)');
    }
    // Check if x86_64 binaries exist
    const x64Present = fs.existsSync(ffmpegX64Dst) && fs.existsSync(ffprobeX64Dst);
    if (x64Present) {
      // Revalidate restored/cached x86_64 binaries like the arm64 pair above.
      verifyChecksum(ffmpegX64Dst, macX64FfmpegBinSha, 'macOS x86_64 ffmpeg (binary)');
      verifyChecksum(ffprobeX64Dst, macX64FfprobeBinSha, 'macOS x86_64 ffprobe (binary)');
    }
    if (arm64Present && x64Present) {
      // Both architectures present - ensure symlinks and exit
      ensureSymlink(ffmpegDst, path.join(distDir, 'ffmpeg-aarch64-apple-darwin'));
      ensureSymlink(ffprobeDst, path.join(distDir, 'ffprobe-aarch64-apple-darwin'));
      console.log('[ensure-ffmpeg-sidecar] ffmpeg/ffprobe sidecars present for both architectures.');
      return;
    }

    if (arm64Present && !x64Present) {
      // ARM64 present but x86_64 missing - download x86_64 only
      ensureSymlink(ffmpegDst, path.join(distDir, 'ffmpeg-aarch64-apple-darwin'));
      ensureSymlink(ffprobeDst, path.join(distDir, 'ffprobe-aarch64-apple-darwin'));
      console.log('[ensure-ffmpeg-sidecar] ARM64 binaries present, downloading x86_64...');

      const tmpX64 = fs.mkdtempSync(path.join(os.tmpdir(), 'fftools-x64-'));

      try {
        download(ffmpegX64Url, path.join(tmpX64, 'ffmpeg.zip'));
        download(ffprobeX64Url, path.join(tmpX64, 'ffprobe.zip'));
        unzip(path.join(tmpX64, 'ffmpeg.zip'), tmpX64);
        unzip(path.join(tmpX64, 'ffprobe.zip'), tmpX64);

        if (fs.existsSync(path.join(tmpX64, 'ffmpeg')) && fs.existsSync(path.join(tmpX64, 'ffprobe'))) {
          verifyChecksum(path.join(tmpX64, 'ffmpeg'), macX64FfmpegBinSha, 'macOS x86_64 ffmpeg (binary)');
          verifyChecksum(path.join(tmpX64, 'ffprobe'), macX64FfprobeBinSha, 'macOS x86_64 ffprobe (binary)');
          fs.copyFileSync(path.join(tmpX64, 'ffmpeg'), ffmpegX64Dst); chmodx(ffmpegX64Dst);
          fs.copyFileSync(path.join(tmpX64, 'ffprobe'), ffprobeX64Dst); chmodx(ffprobeX64Dst);
          console.log('[ensure-ffmpeg-sidecar] Installed x86_64 binaries for Intel Mac support.');
        }
      } catch (err) {
        console.warn(`[ensure-ffmpeg-sidecar] Failed to download x86_64: ${err.message}`);
      } finally {
        cleanupTmp(tmpX64);
      }
      return;
    }

    // Pinned macOS Apple Silicon 8.0 from OSXExperts.NET (override via env if needed)
    const ffmpegZipUrl = process.env.FFMPEG_MAC_URL || 'https://www.osxexperts.net/ffmpeg80arm.zip';
    const ffprobeZipUrl = process.env.FFPROBE_MAC_URL || 'https://www.osxexperts.net/ffprobe80arm.zip';

    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'fftools-'));
    try {
      const ffmpegZip = path.join(tmp, 'ffmpeg.zip');
      const ffprobeZip = path.join(tmp, 'ffprobe.zip');
      download(ffmpegZipUrl, ffmpegZip);
      download(ffprobeZipUrl, ffprobeZip);
      const outFfmpeg = path.join(tmp, 'ffmpeg');
      const outFfprobe = path.join(tmp, 'ffprobe');
      unzip(ffmpegZip, tmp);
      unzip(ffprobeZip, tmp);

      // OSXExperts zips contain a single binary named ffmpeg/ffprobe at tmp
      if (!fs.existsSync(outFfmpeg) || !fs.existsSync(outFfprobe)) {
        fail('Downloaded archives did not contain expected ffmpeg/ffprobe binaries.');
      }
      // Verify checksums against extracted binaries (provider publishes hashes for binaries)
      verifyChecksum(outFfmpeg, macFfmpegBinSha, 'macOS ffmpeg (binary)');
      verifyChecksum(outFfprobe, macFfprobeBinSha, 'macOS ffprobe (binary)');
      fs.copyFileSync(outFfmpeg, ffmpegDst); chmodx(ffmpegDst);
      fs.copyFileSync(outFfprobe, ffprobeDst); chmodx(ffprobeDst);

      ensureSymlink(ffmpegDst, path.join(distDir, 'ffmpeg-aarch64-apple-darwin'));
      ensureSymlink(ffprobeDst, path.join(distDir, 'ffprobe-aarch64-apple-darwin'));
      console.log('[ensure-ffmpeg-sidecar] Installed macOS ARM64 sidecar binaries by download.');
    } finally {
      cleanupTmp(tmp);
    }

    // Also download Intel x86_64 binaries for universal build support
    // Using pinned evermeet.cx 9.0.1 artifacts (trusted source for Intel Mac ffmpeg builds)
    if (!fs.existsSync(ffmpegX64Dst) || !fs.existsSync(ffprobeX64Dst)) {
      console.log('[ensure-ffmpeg-sidecar] Downloading Intel x86_64 binaries for universal support...');

      const tmpX64 = fs.mkdtempSync(path.join(os.tmpdir(), 'fftools-x64-'));
      const ffmpegX64Zip = path.join(tmpX64, 'ffmpeg.zip');
      const ffprobeX64Zip = path.join(tmpX64, 'ffprobe.zip');

      try {
        download(ffmpegX64Url, ffmpegX64Zip);
        download(ffprobeX64Url, ffprobeX64Zip);
        unzip(ffmpegX64Zip, tmpX64);
        unzip(ffprobeX64Zip, tmpX64);

        const outFfmpegX64 = path.join(tmpX64, 'ffmpeg');
        const outFfprobeX64 = path.join(tmpX64, 'ffprobe');

        if (fs.existsSync(outFfmpegX64) && fs.existsSync(outFfprobeX64)) {
          verifyChecksum(outFfmpegX64, macX64FfmpegBinSha, 'macOS x86_64 ffmpeg (binary)');
          verifyChecksum(outFfprobeX64, macX64FfprobeBinSha, 'macOS x86_64 ffprobe (binary)');
          fs.copyFileSync(outFfmpegX64, ffmpegX64Dst); chmodx(ffmpegX64Dst);
          fs.copyFileSync(outFfprobeX64, ffprobeX64Dst); chmodx(ffprobeX64Dst);
          console.log('[ensure-ffmpeg-sidecar] Installed macOS x86_64 sidecar binaries for Intel Mac support.');
        } else {
          console.warn('[ensure-ffmpeg-sidecar] Could not extract x86_64 binaries; Intel Mac builds may fail.');
        }
      } catch (err) {
        console.warn(`[ensure-ffmpeg-sidecar] Failed to download x86_64 binaries: ${err.message}`);
        console.warn('[ensure-ffmpeg-sidecar] Intel Mac builds may fail. ARM64 binaries are still available.');
      } finally {
        cleanupTmp(tmpX64);
      }
    } else {
      console.log('[ensure-ffmpeg-sidecar] x86_64 binaries already present.');
    }

    return;
  }

  if (isWin) {
    const ffmpegDst = path.join(distDir, 'ffmpeg.exe');
    const ffprobeDst = path.join(distDir, 'ffprobe.exe');
    if (fs.existsSync(ffmpegDst) && fs.existsSync(ffprobeDst)) {
      // Tauri externalBin entries are extensionless and require target-triple copies.
      ensureCopy(ffmpegDst, path.join(distDir, 'ffmpeg-x86_64-pc-windows-msvc.exe'));
      ensureCopy(ffprobeDst, path.join(distDir, 'ffprobe-x86_64-pc-windows-msvc.exe'));
      console.log('[ensure-ffmpeg-sidecar] ffmpeg/ffprobe sidecars present. Copies ensured.');
      return;
    }

    // Use gyan.dev essentials latest (or override). This is version-agnostic latest link.
    const zipUrl = process.env.FFMPEG_WIN_URL || 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip';
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'fftools-'));
    try {
      const zipFile = path.join(tmp, 'ffmpeg.zip');
      download(zipUrl, zipFile);
      // Optional checksum verification (provide env or attempt to fetch .sha256 sidecar)
      let winSha = process.env.FFMPEG_WIN_ZIP_SHA256;
      if (!winSha) {
        try {
          const shaFile = path.join(tmp, 'ffmpeg.zip.sha256');
          const shaUrl = zipUrl + '.sha256';
          download(shaUrl, shaFile);
          const txt = fs.readFileSync(shaFile, 'utf8');
          // Typical format: <sha256> *ffmpeg-release-essentials.zip
          winSha = (txt.split(/\s+/)[0] || '').trim();
        } catch (err) {
          // No checksum available from remote and none provided via env.
          // Refuse to install unverified binaries.
          fail(
            'Missing SHA256 for Windows ffmpeg.zip.\n' +
              'Provide FFMPEG_WIN_ZIP_SHA256 env to pin a known archive, or ensure the .sha256 file is accessible.\n' +
              'Example: FFMPEG_WIN_URL + FFMPEG_WIN_ZIP_SHA256.'
          );
        }
      }
      verifyChecksum(zipFile, winSha, 'Windows ffmpeg.zip');
      const outDir = path.join(tmp, 'out');
      unzip(zipFile, outDir);

      // Find bin folder inside extracted structure
      const entries = fs.readdirSync(outDir);
      const root = entries.find((e) => e.toLowerCase().startsWith('ffmpeg-') && fs.statSync(path.join(outDir, e)).isDirectory());
      if (!root) fail('Unexpected archive structure for Windows ffmpeg build.');
      const binDir = path.join(outDir, root, 'bin');
      const srcFfmpeg = path.join(binDir, 'ffmpeg.exe');
      const srcFfprobe = path.join(binDir, 'ffprobe.exe');
      if (!fs.existsSync(srcFfmpeg) || !fs.existsSync(srcFfprobe)) {
        fail('Downloaded Windows archive missing ffmpeg.exe/ffprobe.exe');
      }
      fs.copyFileSync(srcFfmpeg, ffmpegDst);
      fs.copyFileSync(srcFfprobe, ffprobeDst);
      ensureCopy(ffmpegDst, path.join(distDir, 'ffmpeg-x86_64-pc-windows-msvc.exe'));
      ensureCopy(ffprobeDst, path.join(distDir, 'ffprobe-x86_64-pc-windows-msvc.exe'));
      console.log('[ensure-ffmpeg-sidecar] Installed Windows sidecar binaries by download.');
    } finally {
      cleanupTmp(tmp);
    }
    return;
  }

  console.warn('[ensure-ffmpeg-sidecar] Unsupported platform for auto-install; please place binaries under sidecar/ffmpeg/dist/.');
})();
