"""Real-speech native smoke/latency harness; no transcript or path in reports.

Input must be mono 16 kHz PCM16 WAV. For the public JFK fixture use the reference
below; supply --reference for another recording. This measures the native
protocol, not microphone/clipboard end-to-end latency.
"""
import argparse
import array
import base64
import hashlib
import json
import queue
import re
import subprocess
import sys
import threading
import time
import wave


class SidecarClient:
    def __init__(self, executable):
        self.process = subprocess.Popen([executable], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        text=True, encoding="utf-8")
        self.responses = queue.Queue()
        self.request_id = 0
        self.first_partial = None
        self.started = 0
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        try:
            for line in self.process.stdout:
                self.responses.put(json.loads(line))
        finally:
            self.responses.put(None)

    def request(self, kind, **fields):
        self.request_id += 1
        payload = dict(type=kind, id=self.request_id, **fields)
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()
        while True:
            response = self.responses.get(timeout=180)
            if response is None:
                raise RuntimeError("sidecar exited before replying")
            if response["id"] != self.request_id:
                raise RuntimeError("sidecar response ID mismatch")
            if response["type"] == "error":
                raise RuntimeError("sidecar error: " + response["code"])
            if response["type"] != "partial":
                return response
            if self.first_partial is None and (response["committed"] or response["tentative"]):
                self.first_partial = (time.perf_counter() - self.started) * 1000

    def transcribe(self, pcm, mode, language, realtime, step_ms=None):
        self.first_partial = None
        self.started = time.perf_counter()
        options = {} if step_ms is None else {"step_ms": step_ms}
        self.request("start_stream", session_id=self.request_id + 1,
                     mode=mode, language=language, **options)
        session_id = self.request_id
        # 160 ms frames exercise R2T2's own schedule; they do not substitute
        # our own guessed decoder chunk size for the library's recipe.
        for offset in range(0, len(pcm), 2560):
            chunk = array.array("f", pcm[offset:offset + 2560])
            if sys.byteorder != "little":
                chunk.byteswap()
            if realtime:
                time.sleep(max(0, self.started + (offset + len(chunk)) / 16000 - time.perf_counter()))
            response = self.request("audio_chunk", session_id=session_id,
                                    pcm=base64.b64encode(chunk.tobytes()).decode("ascii"))
            if response["samples"] != offset + len(chunk):
                raise RuntimeError("sidecar sample coverage mismatch")
        last_chunk = time.perf_counter()
        result = self.request("finalize_stream", session_id=session_id)
        if result["samples"] != len(pcm):
            raise RuntimeError("final result did not cover complete audio")
        return result, dict(first_partial_ms=self.first_partial,
                            after_last_chunk_ms=(time.perf_counter() - last_chunk) * 1000,
                            total_ms=(time.perf_counter() - self.started) * 1000)

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=10)


def word_error_rate(reference, text):
    expected, actual = (re.findall(r"\w+", value.casefold()) for value in (reference, text))
    # Exact edit distance with one rolling row: O(n*m) time, O(min(n,m)) memory.
    rows, columns = (expected, actual) if len(actual) < len(expected) else (actual, expected)
    previous = list(range(len(columns) + 1))
    for i, word in enumerate(rows, 1):
        current = [i]
        for j, other in enumerate(columns, 1):
            current.append(min(current[-1] + 1, previous[j] + 1, previous[j - 1] + (word != other)))
        previous = current
    return previous[-1] / max(1, len(expected))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sidecar", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--backend", choices=["parakeet", "qwen3"], required=True)
    parser.add_argument("--audio", required=True)
    parser.add_argument("--language", default="auto")
    parser.add_argument("--realtime", action="store_true")
    parser.add_argument("--gpu", action="store_true")
    parser.add_argument("--modes", nargs="+", choices=["batch", "recording"], default=["batch", "recording"])
    parser.add_argument("--reference", required=True)
    parser.add_argument("--max-wer", type=float, default=0.1)
    parser.add_argument("--step-ms", type=int)
    args = parser.parse_args()
    with open(args.model, "rb") as model:
        if hashlib.file_digest(model, "sha256").hexdigest() != args.sha256:
            raise RuntimeError("model SHA-256 mismatch")
    with wave.open(args.audio, "rb") as audio:
        if (audio.getnchannels(), audio.getframerate(), audio.getsampwidth()) != (1, 16000, 2):
            raise RuntimeError("fixture must be mono 16 kHz PCM16 WAV")
        pcm16 = array.array("h", audio.readframes(audio.getnframes()))
        if sys.byteorder != "little":
            pcm16.byteswap()
        pcm = [value / 32768 for value in pcm16]
    client = SidecarClient(args.sidecar)
    try:
        started = time.perf_counter()
        client.request("load_model", model="smoke", model_path=args.model,
                       backend=args.backend, gpu=args.gpu, threads=4)
        load_ms = (time.perf_counter() - started) * 1000
        reused = client.request("load_model", model="smoke", model_path=args.model,
                                backend=args.backend, gpu=args.gpu, threads=4)
        if not reused["reused"]:
            raise RuntimeError("warm model was loaded again")
        report = dict(backend=args.backend, gpu_requested=args.gpu, load_ms=load_ms,
                      duration_ms=len(pcm) / 16, model_hash_verified=True, model_sha256=args.sha256)
        for mode in args.modes:
            result, timings = client.transcribe(pcm, mode, args.language, args.realtime and mode == "recording", args.step_ms)
            report[mode] = dict(**timings, chars=len(result["text"]),
                                stream_retry=result.get("stream_retry", False),
                                wer=word_error_rate(args.reference, result["text"]))
        print(json.dumps(report, indent=2))
        if any(report[mode]["wer"] > args.max_wer for mode in args.modes):
            raise RuntimeError("real-speech accuracy acceptance failed")
    finally:
        client.close()


if __name__ == "__main__":
    main()
