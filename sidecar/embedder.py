import os
import threading
from typing import ClassVar

# Enforce strict CPU thread limits to prevent PyTorch from saturating all laptop cores
os.environ["OMP_NUM_THREADS"] = "2"
os.environ["MKL_NUM_THREADS"] = "2"

try:
    import torch
    torch.set_num_threads(2)
except Exception:
    pass

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
        try:
            import torch
            torch.set_num_threads(2)
        except Exception:
            pass
        self.model = SentenceTransformer("all-MiniLM-L6-v2", device="cpu")
        self._initialized = True
        print("Embedding model loaded (CPU threads clamped to 2)", flush=True)

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
