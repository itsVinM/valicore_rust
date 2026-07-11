import math
import pytest
from valicore.core import RustSignalProcessor


@pytest.fixture
def sine_wave() -> tuple[list[float], int]:
    sample_rate : int = 1000 # Hz
    frequency : int = 50 # Hz
    num_samples : int = 1024
    time : list[float] = [idx / sample_rate for idx in range(num_samples)]
    samples : list[float] = [math.sin(2 * math.pi * frequency * time_sample) for time_sample in time]
    return samples, sample_rate


class TestRustSignalProcessor:
    """
    Test suite for the RustSignalProcessor class.
    """
    def test_fft(self, sine_wave) -> None:
        samples, sample_rate = sine_wave
        result = RustSignalProcessor.fft(samples, sample_rate)
        assert "frequencies_hz" in result
        assert "magnitudes" in result
        assert len(result["frequencies_hz"]) == len(samples) // 2
        assert len(result["magnitudes"]) == len(samples) // 2

        peak_idx = result["magnitudes"].index(max(result["magnitudes"]))
        peak_freq = result["frequencies_hz"][peak_idx]
        assert abs(peak_freq - 50.0) < 2.0

    def test_psd(self, sine_wave) -> None:
        samples, sample_rate = sine_wave
        result = RustSignalProcessor.psd(samples, sample_rate)
        assert "frequencies_hz" in result
        assert "power_density" in result

    def test_stats(self, sine_wave) -> None:
        samples, _ = sine_wave
        stats = RustSignalProcessor.stats(samples)
        assert "mean" in stats
        assert "std" in stats
        assert "min" in stats
        assert "max" in stats
        assert "rms" in stats
        assert "crest_factor" in stats
        assert abs(stats["mean"]) < 0.05
        assert abs(stats["rms"] - 0.707) < 0.02

    def test_thd(self, sine_wave) -> None:
        samples, sample_rate = sine_wave
        thd_val = RustSignalProcessor.thd(samples, 50.0, sample_rate)
        assert thd_val < 5.0

    def test_window_hann(self, sine_wave) -> None:
        samples, _ = sine_wave
        windowed = RustSignalProcessor.window(samples, "hann")
        assert len(windowed) == len(samples)
        assert abs(windowed[0]) < 0.001
        assert abs(windowed[len(samples) // 2]) > 0.5

    def test_window_hamming(self, sine_wave) -> None:
        samples, _ = sine_wave
        windowed = RustSignalProcessor.window(samples, "hamming")
        assert len(windowed) == len(samples)

    def test_window_blackman(self, sine_wave) -> None:
        samples, _ = sine_wave
        windowed = RustSignalProcessor.window(samples, "blackman")
        assert len(windowed) == len(samples)

    def test_window_invalid(self, sine_wave) -> None:
        samples, _ = sine_wave
        with pytest.raises(ValueError):
            RustSignalProcessor.window(samples, "nonexistent")

    def test_filter_lowpass(self, sine_wave) -> None:
        samples, _ = sine_wave
        filtered = RustSignalProcessor.filter(samples, "lowpass", 0.5, 2)
        assert len(filtered) == len(samples)

    def test_filter_highpass(self, sine_wave) -> None:
        samples, _ = sine_wave
        filtered = RustSignalProcessor.filter(samples, "highpass", 0.1, 2)
        assert len(filtered) == len(samples)

    def test_filter_invalid_type(self, sine_wave) -> None:
        samples, _ = sine_wave
        with pytest.raises(ValueError):
            RustSignalProcessor.filter(samples, "bandpass", 0.5, 2)

    def test_cross_correlate(self, sine_wave) -> None:
        samples, _ = sine_wave
        result = RustSignalProcessor.cross_correlate(samples, samples)
        assert len(result) == len(samples)
        assert abs(result[0]) > abs(result[len(samples) // 2])

    def test_moving_average(self, sine_wave) -> None:
        samples, _ = sine_wave
        result = RustSignalProcessor.moving_average(samples, 5)
        assert len(result) == len(samples)

    def test_empty_input(self) -> None:
        with pytest.raises(ValueError):
            RustSignalProcessor.fft([], 1000.0)

    def test_single_value(self) -> None:
        result = RustSignalProcessor.stats([42.0])
        assert result["mean"] == 42.0
        assert result["count"] == 1.0
