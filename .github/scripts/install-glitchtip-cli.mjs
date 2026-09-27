import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const VERSION = "1.0.0";
const CRATE_URL = `https://static.crates.io/crates/glitchtip-cli/glitchtip-cli-${VERSION}.crate`;
const CRATE_SHA256 = "8ca61cf0817e85ccedb6c378e725e896dbf76d18bfa89fd1476f38c6bb882906";

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`${command} exited with status ${result.status}`);
  }
}

const UPLOADER_REPLACEMENTS = [
  [
    String.raw`    let upload_info = client
        .get_chunk_upload_info(org)
        .await
        .context("Failed to get chunk upload info")?;

    let mut uploaded = 0;`,
    String.raw`    let upload_info = client
        .get_chunk_upload_info(org)
        .await
        .context("Failed to get chunk upload info")?;
    let chunk_size = usize::try_from(upload_info.chunk_size)
        .context("Server chunk size does not fit this platform")?;
    if chunk_size == 0 {
        anyhow::bail!("Server advertised a zero-byte chunk size");
    }

    let mut uploaded = 0;`,
  ],
  [
    String.raw`        // Gzip compress
        let compressed = gzip_compress(&data)?;
        // Upload chunk
        match client
            .upload_chunk(&upload_info.url, compressed, &checksum)
            .await
        {
            Ok(()) => {
                println!("  Uploaded chunk for: {display_name}");
                uploaded += 1;
            }
            Err(e) => {
                eprintln!("  Error uploading {display_name}: {e}");
                errors += 1;
                continue;
            }
        }`,
    String.raw`        let chunks = data.chunks(chunk_size);
        let chunk_count = chunks.len();
        let mut chunk_checksums = Vec::with_capacity(chunk_count);
        let mut upload_failed = false;

        for (index, chunk) in chunks.enumerate() {
            let chunk_checksum = sha1_hex(chunk);
            let compressed = gzip_compress(chunk)?;
            match client
                .upload_chunk(&upload_info.url, compressed, &chunk_checksum)
                .await
            {
                Ok(()) => {
                    println!(
                        "  Uploaded chunk {}/{} for: {display_name}",
                        index + 1,
                        chunk_count
                    );
                    uploaded += 1;
                    chunk_checksums.push(chunk_checksum);
                }
                Err(e) => {
                    eprintln!("  Error uploading {display_name}: {e}");
                    errors += 1;
                    upload_failed = true;
                    break;
                }
            }
        }

        if upload_failed {
            continue;
        }`,
  ],
  [
    "                chunks: vec![checksum],",
    "                chunks: chunk_checksums,",
  ],
  [
    String.raw`    if assemble_files.is_empty() {
        println!("No files to assemble.");
        return Ok(());
    }`,
    String.raw`    if assemble_files.is_empty() {
        println!("No files to assemble.");
        if errors > 0 {
            anyhow::bail!("{errors} file(s) had errors");
        }
        return Ok(());
    }`,
  ],
];

async function patchUploader(path) {
  let source = await readFile(path, "utf8");
  for (const [before, after] of UPLOADER_REPLACEMENTS) {
    const first = source.indexOf(before);
    if (first < 0 || source.indexOf(before, first + before.length) >= 0) {
      throw new Error("Pinned glitchtip-cli source did not contain one exact patch target");
    }
    source = `${source.slice(0, first)}${after}${source.slice(first + before.length)}`;
  }
  await writeFile(path, source);
}

const workspace = await mkdtemp(join(tmpdir(), "glitchtip-cli-"));

try {
  const response = await fetch(CRATE_URL, {
    headers: { "user-agent": "VoiceTypr release workflow" },
  });
  if (!response.ok) {
    throw new Error(`Failed to download ${CRATE_URL}: HTTP ${response.status}`);
  }

  const archive = Buffer.from(await response.arrayBuffer());
  const actualSha256 = createHash("sha256").update(archive).digest("hex");
  if (actualSha256 !== CRATE_SHA256) {
    throw new Error(
      `glitchtip-cli crate checksum mismatch: expected ${CRATE_SHA256}, got ${actualSha256}`,
    );
  }

  const archivePath = join(workspace, `glitchtip-cli-${VERSION}.crate`);
  await writeFile(archivePath, archive);
  run("tar", ["-xzf", archivePath, "-C", workspace], workspace);

  const sourcePath = join(workspace, `glitchtip-cli-${VERSION}`);
  await patchUploader(join(sourcePath, "src", "commands", "debug_files.rs"));
  run("cargo", ["install", "--path", sourcePath, "--locked", "--force"], sourcePath);
} finally {
  await rm(workspace, { recursive: true, force: true });
}
