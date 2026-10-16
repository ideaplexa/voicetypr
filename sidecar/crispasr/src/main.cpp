#include "session.h"
#include <cstdio>
#include <iostream>
#include <array>
#include <string_view>
#ifdef _WIN32
#include <io.h>
#else
#include <unistd.h>
#endif

int main() {
    // The native runtimes can print file names or decoder diagnostics. Keep a
    // private copy of stdout for protocol writes and silence their stdio before
    // constructing any model. Errors cross IPC only as closed, content-free codes.
#ifdef _WIN32
    FILE* protocol = _fdopen(_dup(_fileno(stdout)), "w");
    std::freopen("NUL", "w", stdout);
    std::freopen("NUL", "w", stderr);
#else
    FILE* protocol = fdopen(dup(fileno(stdout)), "w");
    std::freopen("/dev/null", "w", stdout);
    std::freopen("/dev/null", "w", stderr);
#endif
    if (!protocol) return 1;
    auto emit = [protocol](const voicetypr::Json& message) {
        const auto line = message.dump() + "\n";
        if (std::fwrite(line.data(), 1, line.size(), protocol) != line.size() || std::fflush(protocol) != 0)
            std::exit(1);
    };
    voicetypr::Session session(emit);
    for (std::string line; std::getline(std::cin, line);) {
        uint64_t id = 0;
        try {
            if (line.size() > 1024 * 1024) throw std::invalid_argument("invalid_request");
            const auto request = voicetypr::Json::parse(line);
            id = request.at("id").get<uint64_t>();
            session.dispatch(request);
            if (request.at("type") == "shutdown") break;
        } catch (const std::invalid_argument& error) {
            // Only our closed protocol codes may cross the boundary. Native
            // exceptions are not trusted diagnostics: their what() can contain
            // a model path or input text.
            constexpr std::array<std::string_view, 14> codes = {
                "invalid_request", "invalid_audio", "unknown_command", "stream_busy",
                "model_not_found", "unsupported_backend", "model_load_failed", "model_not_loaded",
                "stream_unavailable", "session_mismatch", "stream_limit", "decode_failed", "finalize_failed", "gpu_unavailable"};
            const std::string_view code(error.what());
            emit({{"type", "error"}, {"id", id}, {"code", std::find(codes.begin(), codes.end(), code) != codes.end() ? std::string(code) : "decode_failed"}});
        } catch (...) {
            emit({{"type", "error"}, {"id", id}, {"code", "invalid_request"}});
        }
    }
    std::fclose(protocol);
}
