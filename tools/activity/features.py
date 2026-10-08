"""Feature contract v1. ASCII only; no dependencies; mirrored in capture/activity.rs.

Inputs here must already be privacy-sanitized. Training uses only invented text.
Binary hashed features: shared/field words, field bigrams, shared token trigrams.
FNV-1a u32 UTF-8 with wrapping; feature index = hash % 8192. First 2048
distinct features in emission order, then sorted. L2 normalization at inference.
"""

import math

DIMENSIONS = 8192
CATEGORIES = [
    "coding_debugging", "research_learning", "writing_documentation",
    "communication", "planning_administration", "entertainment_media",
    "system_navigation", "other",
]
FIELDS = (("app", 16, 256), ("title", 64, 512), ("url", 64, 2048),
          ("content", 128, 4096))
MAX_FEATURES = 2048


def fnv1a(text):
    result = 2166136261
    for byte in text.encode("utf-8"):
        result = ((result ^ byte) * 16777619) & 0xffffffff
    return result


def tokens(text, limit):
    # Non-ASCII characters delimit, rather than language-dependent case folding.
    words = []
    current = []
    for char in text:
        if char.isascii() and char.isalnum():
            if len(current) < 32:
                current.append(char.lower())
        elif current:
            words.append("".join(current))
            current = []
            if len(words) == limit:
                return words
    if current and len(words) < limit:
        words.append("".join(current))
    return words


def indices(fields):
    result = set()

    def emit(feature):
        if len(result) < MAX_FEATURES:
            result.add(fnv1a(feature) % DIMENSIONS)

    for name, limit, chars in FIELDS:
        previous = None
        for word in tokens((fields.get(name) or "")[:chars], limit):
            emit("w:" + word)
            emit(name + ":w:" + word)
            if previous is not None:
                emit(name + ":b:" + previous + " " + word)
            marked = "^" + word + "$"
            for start in range(len(marked) - 2):
                emit("c:" + marked[start:start + 3])
            previous = word
    return sorted(result)


def features(fields):
    active = indices(fields)
    value = 1.0 / math.sqrt(len(active)) if active else 0.0
    return [(index, value) for index in active]
