#pragma once
#include <array>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <stdexcept>
#include <string>
#include <vector>

namespace voicetypr {
constexpr size_t sample_rate = 16'000;

// PCM crosses IPC as little-endian float32, preserving the streaming
// resampler's output without a lossy f32 -> i16 -> f32 round trip.
inline std::vector<float> decode_audio(const std::string& input) {
    if (input.size() > 4 * ((sample_rate * sizeof(float) + 2) / 3) || input.size() % 4 != 0)
        throw std::invalid_argument("invalid_audio");
    static const auto alphabet = [] {
        std::array<int, 256> values{};
        values.fill(-1);
        const std::string chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for (size_t i = 0; i < chars.size(); ++i) values[static_cast<unsigned char>(chars[i])] = static_cast<int>(i);
        return values;
    }();
    std::vector<uint8_t> bytes;
    bytes.reserve(input.size() / 4 * 3);
    for (size_t i = 0; i < input.size(); i += 4) {
        uint32_t value = 0;
        size_t padding = 0;
        for (size_t j = 0; j < 4; ++j) {
            const unsigned char ch = static_cast<unsigned char>(input[i + j]);
            if (ch == '=') {
                if (j < 2 || i + 4 != input.size()) throw std::invalid_argument("invalid_audio");
                ++padding;
                value <<= 6;
            } else {
                if (padding || alphabet[ch] < 0) throw std::invalid_argument("invalid_audio");
                value = (value << 6) | static_cast<uint32_t>(alphabet[ch]);
            }
        }
        if ((padding == 1 && (value & 0xff) != 0) || (padding == 2 && (value & 0xffff) != 0))
            throw std::invalid_argument("invalid_audio");
        bytes.push_back(static_cast<uint8_t>(value >> 16));
        if (padding < 2) bytes.push_back(static_cast<uint8_t>(value >> 8));
        if (padding == 0) bytes.push_back(static_cast<uint8_t>(value));
    }
    if (bytes.size() % sizeof(float)) throw std::invalid_argument("invalid_audio");
    std::vector<float> samples(bytes.size() / sizeof(float));
    for (size_t i = 0; i < samples.size(); ++i) {
        uint32_t bits = 0;
        for (size_t j = 0; j < 4; ++j) bits |= static_cast<uint32_t>(bytes[i * 4 + j]) << (8 * j);
        std::memcpy(&samples[i], &bits, sizeof(float));
        // A finite resampler overshoot is valid audio, not a corrupt frame.
        // Preserve it rather than rejecting loud takes or silently clipping.
        if (!std::isfinite(samples[i]))
            throw std::invalid_argument("invalid_audio");
    }
    return samples;
}
}
