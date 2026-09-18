from __future__ import annotations

import threading
from typing import ClassVar

import numpy as np
from sentence_transformers import SentenceTransformer


class Embedder:
    _instance: ClassVar["Embedder | None"] = None
    _lock: ClassVar[threading.Lock] = threading.Lock()
    _initialized: bool

    def __new__(cls) -> "Embedder":
        with cls._lock:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
                cls._instance._initialized = False
            return cls._instance

    def __init__(self) -> None:
        if self._initialized:
            return
        self.model = SentenceTransformer("all-MiniLM-L6-v2", device="cpu")
        self._initialized = True
        print("Embedding model loaded", flush=True)

    def embed(self, text: str) -> list[float]:
        vector = self.model.encode(text or "", normalize_embeddings=True)
        return np.asarray(vector, dtype=float).tolist()

    def embed_batch(self, texts: list[str]) -> list[list[float]]:
        vectors = self.model.encode(texts, normalize_embeddings=True, batch_size=32)
        return np.asarray(vectors, dtype=float).tolist()

    def cosine_similarity(self, a: list[float], b: list[float]) -> float:
        a_np = np.asarray(a, dtype=float)
        b_np = np.asarray(b, dtype=float)
        denominator = float(np.linalg.norm(a_np) * np.linalg.norm(b_np))
        if denominator == 0.0:
            return 0.0
        return max(-1.0, min(1.0, float(np.dot(a_np, b_np) / denominator)))
