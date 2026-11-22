#!/usr/bin/env python3
"""TaskFlow Client-Side Zero-Knowledge Encryption Engine.

Provides:
- End-to-End Encryption (E2EE) for cloud mirrors (Supabase)
- Authenticated AES-128-CBC + HMAC-SHA256 encryption via Fernet
- Deterministic HMAC-SHA256 blinding for foreign key identifiers
- Graceful backward compatibility (plain text fallback if unencrypted)
"""
from __future__ import annotations

import base64
import hashlib
import hmac
import json
import os
import sqlite3
import sys
from pathlib import Path
from typing import Any, Optional

try:
    from cryptography.fernet import Fernet, InvalidToken
    CRYPTOGRAPHY_AVAILABLE = True
except ImportError:
    CRYPTOGRAPHY_AVAILABLE = False
    Fernet = None  # type: ignore
    InvalidToken = Exception  # type: ignore


CIPHER_PREFIX = "enc:v1:"


def get_default_db_path() -> Path:
    """Resolve default TaskFlow SQLite database path."""
    if "TASKFLOW_DB" in os.environ:
        return Path(os.environ["TASKFLOW_DB"])
    if sys.platform == "win32":
        appdata = os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "com.taskflow.desktop" / "taskflow.sqlite"
    else:
        return Path.home() / ".local" / "share" / "com.taskflow.desktop" / "taskflow.sqlite"
    return Path("taskflow.sqlite")


def get_vault_key() -> bytes:
    """Retrieve or generate the local client-side Vault Encryption Key.
    
    Resolution order:
    1. TASKFLOW_VAULT_KEY or TASKFLOW_ENCRYPTION_KEY environment variable.
    2. .env files in project root or mcp-cloud/.
    3. SQLite settings table (`vault_encryption_key`).
    4. Deterministic machine fallback key derived from host + user identity.
    """
    # 1. Check environment variables
    env_key = os.environ.get("TASKFLOW_VAULT_KEY") or os.environ.get("TASKFLOW_ENCRYPTION_KEY")
    if env_key:
        return _normalize_to_fernet_key(env_key)

    # 2. Check .env files
    search_paths = [
        Path(".env"),
        Path("mcp-cloud/.env"),
        Path(__file__).parent / ".env",
        Path(__file__).parent.parent / ".env",
    ]
    for env_path in search_paths:
        if env_path.exists():
            try:
                for line in env_path.read_text(encoding="utf-8").splitlines():
                    line = line.strip()
                    if line.startswith("TASKFLOW_VAULT_KEY=") or line.startswith("TASKFLOW_ENCRYPTION_KEY="):
                        val = line.split("=", 1)[1].strip().strip('"').strip("'")
                        if val:
                            return _normalize_to_fernet_key(val)
            except Exception:
                pass

    # 3. Check / store in SQLite settings
    db_path = get_default_db_path()
    if db_path.exists():
        try:
            con = sqlite3.connect(str(db_path))
            cur = con.cursor()
            cur.execute("SELECT value FROM settings WHERE key = 'vault_encryption_key'")
            row = cur.fetchone()
            if row and row[0]:
                con.close()
                return _normalize_to_fernet_key(row[0])

            # Generate new random Fernet key and save it in SQLite settings
            if CRYPTOGRAPHY_AVAILABLE:
                new_key = Fernet.generate_key().decode("utf-8")
                cur.execute("""
                    INSERT INTO settings (key, value) VALUES ('vault_encryption_key', ?)
                    ON CONFLICT(key) DO UPDATE SET value = excluded.value
                """, (new_key,))
                con.commit()
                con.close()
                return new_key.encode("utf-8")
            con.close()
        except Exception:
            pass

    # 4. Deterministic machine fallback key if SQLite is unreachable
    return _derive_machine_fallback_key()


def _normalize_to_fernet_key(raw_key: str | bytes) -> bytes:
    """Ensure key is a valid 32-byte urlsafe base64-encoded Fernet key."""
    if isinstance(raw_key, str):
        raw_bytes = raw_key.encode("utf-8")
    else:
        raw_bytes = raw_key

    # Already valid Fernet key length (44 base64 chars)
    if len(raw_bytes) == 44:
        try:
            base64.urlsafe_b64decode(raw_bytes)
            return raw_bytes
        except Exception:
            pass

    # Hash arbitrary secret string to 32 bytes and base64-encode
    hashed = hashlib.sha256(raw_bytes).digest()
    return base64.urlsafe_b64encode(hashed)


def _derive_machine_fallback_key() -> bytes:
    """Derive a reproducible local machine key from hostname and username."""
    hostname = os.environ.get("COMPUTERNAME") or os.environ.get("HOSTNAME") or "taskflow-local"
    username = os.environ.get("USERNAME") or os.environ.get("USER") or "default-user"
    seed = f"taskflow-vault:{hostname}:{username}".encode("utf-8")
    return base64.urlsafe_b64encode(hashlib.sha256(seed).digest())


def encrypt_text(plaintext: Optional[str], key: Optional[bytes] = None) -> str:
    """Encrypt a plaintext string into a prefixed Fernet ciphertext token.
    
    If text is empty or None, returns an empty string.
    If cryptography library is not installed, returns the plaintext.
    """
    if plaintext is None or plaintext == "":
        return ""

    # Don't double encrypt
    if plaintext.startswith(CIPHER_PREFIX):
        return plaintext

    if not CRYPTOGRAPHY_AVAILABLE:
        return plaintext

    try:
        active_key = key or get_vault_key()
        f = Fernet(active_key)
        token = f.encrypt(plaintext.encode("utf-8")).decode("utf-8")
        return f"{CIPHER_PREFIX}{token}"
    except Exception:
        return plaintext


def decrypt_text(ciphertext: Optional[str], key: Optional[bytes] = None) -> str:
    """Decrypt a ciphertext token back into plaintext.
    
    If string is not encrypted (e.g. legacy plaintext or missing prefix),
    returns the original string unchanged.
    """
    if ciphertext is None or ciphertext == "":
        return ""

    if not CRYPTOGRAPHY_AVAILABLE:
        return ciphertext

    # Check for prefix or raw Fernet token
    raw_token = ciphertext
    if ciphertext.startswith(CIPHER_PREFIX):
        raw_token = ciphertext[len(CIPHER_PREFIX):]
    elif not ciphertext.startswith("gAAAAA"):
        # Not a Fernet ciphertext, return plain text
        return ciphertext

    try:
        active_key = key or get_vault_key()
        f = Fernet(active_key)
        decrypted = f.decrypt(raw_token.encode("utf-8")).decode("utf-8")
        return decrypted
    except (InvalidToken, Exception):
        # Fallback to returning raw string if token is invalid or key mismatch
        return ciphertext


def encrypt_json(data: Any, key: Optional[bytes] = None) -> Any:
    """Serialize any JSON-compatible structure, encrypt it, and return as a protected container."""
    if data is None:
        return []
    try:
        json_str = json.dumps(data)
        enc_token = encrypt_text(json_str, key=key)
        return [enc_token]
    except Exception:
        return data


def decrypt_json(data: Any, key: Optional[bytes] = None) -> Any:
    """Decrypt a protected JSON container back into its original Python object."""
    if data is None:
        return []

    if isinstance(data, list) and len(data) == 1 and isinstance(data[0], str) and (data[0].startswith(CIPHER_PREFIX) or data[0].startswith("gAAAAA")):
        decrypted_str = decrypt_text(data[0], key=key)
        try:
            return json.loads(decrypted_str)
        except Exception:
            return decrypted_str

    if isinstance(data, str) and (data.startswith(CIPHER_PREFIX) or data.startswith("gAAAAA")):
        decrypted_str = decrypt_text(data, key=key)
        try:
            return json.loads(decrypted_str)
        except Exception:
            return decrypted_str

    return data


def blind_identifier(identifier: str, key: Optional[bytes] = None) -> str:
    """Generate a deterministic, pseudonymized HMAC-SHA256 representation of an identifier.
    
    Used for non-reversible indexing / matching on the server side without leaking the plain string.
    """
    if not identifier:
        return ""
    active_key = key or get_vault_key()
    h = hmac.new(active_key, identifier.encode("utf-8"), hashlib.sha256).hexdigest()
    return f"blind_{h[:24]}"
