from __future__ import annotations

from typing import Any

import numpy as np

# ── Rust extension (primary) ──────────────────────────────────

try:
    from valicore._rust import (
        Oscilloscope as _RustOscilloscope,
        apply_filter as _apply_filter,
        apply_window as _apply_window,
        compute_fft as _compute_fft,
        compute_psd as _compute_psd,
        compute_stats as _compute_stats,
        compute_thd as _compute_thd,
        cross_correlate as _cross_correlate,
    )

    _HAS_RUST = True
except ImportError:
    _HAS_RUST = False

# ── Python fallback (when Rust extension unavailable) ──────────

if not _HAS_RUST:
    from valicore.driver._fallback import ScopeFallback as _PythonOscilloscope


# ── Unified Oscilloscope export ────────────────────────────────

def Oscilloscope(brand: str, timeout_ms: int | None = None):  # noqa: N802
    """Create an oscilloscope driver.

    Returns the Rust Oscilloscope when the compiled extension is available,
    otherwise falls back to the pyvisa-based Python driver.
    """
    if _HAS_RUST:
        return _RustOscilloscope(brand, timeout_ms)
    return _PythonOscilloscope(brand, timeout_ms)


# ── Signal processing (Rust-only, no fallback needed) ──────────

class RustSignalProcessor:
    """High-performance signal processing backed by Rust via PyO3."""

    @staticmethod
    def fft(samples: list[float], sample_rate: float) -> dict[str, list[float]]:
        freqs, mags = _compute_fft(samples, sample_rate)
        return {"frequencies_hz": freqs, "magnitudes": mags}

    @staticmethod
    def psd(samples: list[float], sample_rate: float) -> dict[str, list[float]]:
        freqs, power = _compute_psd(samples, sample_rate)
        return {"frequencies_hz": freqs, "power_density": power}

    @staticmethod
    def stats(samples: list[float]) -> dict[str, float]:
        return dict(_compute_stats(samples))

    @staticmethod
    def window(samples: list[float], window_type: str = "hann") -> list[float]:
        return _apply_window(samples, window_type)

    @staticmethod
    def filter(
        samples: list[float],
        filter_type: str = "lowpass",
        cutoff: float = 0.5,
        order: int = 2,
    ) -> list[float]:
        return _apply_filter(samples, filter_type, cutoff, order)

    @staticmethod
    def thd(samples: list[float], fundamental_hz: float, sample_rate: float) -> float:
        return _compute_thd(samples, fundamental_hz, sample_rate)

    @staticmethod
    def cross_correlate(a: list[float], b: list[float]) -> list[float]:
        return _cross_correlate(a, b)

    @staticmethod
    def moving_average(samples: list[float], window_size: int = 5) -> list[float]:
        if window_size < 1:
            raise ValueError("window_size must be >= 1")
        arr = np.asarray(samples, dtype=np.float64)
        kernel = np.ones(window_size, dtype=np.float64) / window_size
        return np.convolve(arr, kernel, mode="same").tolist()
