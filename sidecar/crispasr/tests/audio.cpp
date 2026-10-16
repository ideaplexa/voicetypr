#include "audio.h"
#include <iostream>

int main() {
    const auto samples = voicetypr::decode_audio("AACAPwAAAL8=");
    if (samples != std::vector<float>{1.0f, -0.5f}) return 1;
    // Band-limited resampling can overshoot unity even for valid PCM16 input.
    if (voicetypr::decode_audio("AACgPw==") != std::vector<float>{1.25f}) return 4;
    if (!voicetypr::decode_audio("").empty()) return 2;
    for (const std::string invalid : {"???=", "AAAAA", "AA==AAAA", "AAAA", "AADAfw==", "AB=="}) {
        try { voicetypr::decode_audio(invalid); return 3; }
        catch (const std::invalid_argument&) { }
    }
    std::cout << "PCM transport checks passed\n";
}
