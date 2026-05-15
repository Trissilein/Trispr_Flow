// Model descriptions constant
export const MODEL_DESCRIPTIONS: Record<
    string,
    { summary: string; speed: string; accuracy: string; languages: string }
> = {
    "whisper-large-v3": {
        summary: "Best overall quality. Largest model with highest accuracy.",
        speed: "Slowest",
        accuracy: "Highest",
        languages: "Multilingual",
    },
    "whisper-large-v3-turbo": {
        summary: "Speed-optimized large model with strong accuracy.",
        speed: "Very fast",
        accuracy: "High",
        languages: "Multilingual",
    },
    "ggml-distil-large-v3": {
        summary: "Distilled variant focused on speed with near‑large quality.",
        speed: "Fastest",
        accuracy: "High (EN‑focused)",
        languages: "Primarily English",
    },
    "ggml-large-v3-turbo-q5_0": {
        summary: "Quantized large-v3-turbo for smaller size and faster runtime.",
        speed: "Fast",
        accuracy: "Slightly lower",
        languages: "Multilingual",
    },
    "ggml-large-v3-turbo-q8_0": {
        summary: "Q8 quantized large-v3-turbo with higher fidelity than q5 at larger size.",
        speed: "Moderate",
        accuracy: "High",
        languages: "Multilingual",
    },
    "ggml-large-v3-q5_0": {
        summary: "Quantized large-v3 for balanced quality and size reduction.",
        speed: "Moderately slow",
        accuracy: "Slightly lower than full",
        languages: "Multilingual",
    },
    "ggml-large-v3-q8_0": {
        summary: "Q8 quantized large-v3 prioritizing quality with moderate size reduction.",
        speed: "Slow",
        accuracy: "High",
        languages: "Multilingual",
    },
    "ggml-distil-large-v3-q5_0": {
        summary: "Quantized distil model for ultra-fast inference with minimal accuracy loss.",
        speed: "Fastest",
        accuracy: "Medium (EN-focused)",
        languages: "Primarily English",
    },
    "ggml-distil-large-v3-q8_0": {
        summary: "Q8 quantized distil model with better wording stability than q5.",
        speed: "Fast",
        accuracy: "Medium-high (EN-focused)",
        languages: "Primarily English",
    },
    "ggml-large-v3-turbo-german-q5_0": {
        summary: "Quantized German-optimized model for efficient German speech recognition.",
        speed: "Fast",
        accuracy: "Slightly lower",
        languages: "German-optimized",
    },
    "ggml-large-v3-turbo-german-q8_0": {
        summary: "Q8 German-optimized model for higher quality German speech recognition.",
        speed: "Moderate",
        accuracy: "High",
        languages: "German-optimized",
    },
};
