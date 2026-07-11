"""Python fallback oscilloscope driver.

Wraps ScopeAutomation (pyvisa) to match the Rust Oscilloscope API.
Used when the compiled Rust extension is unavailable.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Optional

from valicore.driver._oscilloscope_driver import (
    ScopeAutomation,
    ScopeConnectionError,
    ScopeConfigurationError,
)

_YAML_PATH = Path(__file__).parent / "oscilloscope.yaml"


class ScopeFallback:
    """Drop-in replacement for the Rust Oscilloscope using pyvisa."""

    def __init__(self, brand: str, timeout_ms: Optional[int] = None) -> None:
        self._scope = ScopeAutomation(path=_YAML_PATH, brand=brand)
        self._brand = brand
        self._connected = False
        self._ip: Optional[str] = None
        self._port: int = 5025
        self._instrument_id = "OFFLINE"

    @staticmethod
    def brands() -> list[str]:
        scope = ScopeAutomation(path=_YAML_PATH, brand="RS")
        return sorted(scope.lib.keys())

    @staticmethod
    def available_settings() -> list[str]:
        from valicore.driver._oscilloscope_driver import _SETTINGS
        return sorted(_SETTINGS.keys())

    @staticmethod
    def available_gettings() -> list[str]:
        from valicore.driver._oscilloscope_driver import _GETTERS
        return sorted(_GETTERS.keys())

    @staticmethod
    def detect_brand(addr: str, port: Optional[int] = None, timeout_ms: Optional[int] = None) -> str:
        import socket
        probe_port = port or 5025
        timeout_s = (timeout_ms or 5000) / 1000.0
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(timeout_s)
        try:
            sock.connect((addr, probe_port))
            sock.sendall(b"*IDN?\n")
            data = sock.recv(4096).decode().strip()
        except Exception:
            data = ""
        finally:
            sock.close()

        for brand in ScopeFallback.brands():
            scope = ScopeAutomation(path=_YAML_PATH, brand=brand)
            idn_pattern = scope.lib[brand].get("idn_pattern", "")
            if idn_pattern and idn_pattern.upper() in data.upper():
                return brand
        raise ScopeConnectionError(f"no brand matches *IDN? response: {data}")

    @staticmethod
    def from_ip(addr: str, port: Optional[int] = None, timeout_ms: Optional[int] = None) -> "ScopeFallback":
        probe_port = port or 5025
        brand = ScopeFallback.detect_brand(addr, probe_port, timeout_ms)
        fallback = ScopeFallback(brand=brand, timeout_ms=timeout_ms)
        fallback._ip = addr
        fallback._port = port or fallback._scope.lib[brand].get("default_port", 5025)
        fallback.connect(addr, fallback._port)
        return fallback

    def brand(self) -> str:
        return self._brand

    def default_port(self) -> int:
        return self._port

    def is_connected(self) -> bool:
        return self._connected

    def active_channels(self) -> list[int]:
        if not self._connected:
            return []
        chs = []
        for ch in range(1, 5):
            try:
                if self.check_ch(str(ch)):
                    chs.append(ch)
            except Exception:
                break
        return chs

    def instrument_id(self) -> str:
        return self._instrument_id

    def connect(self, addr: str, port: Optional[int] = None) -> None:
        self._ip = addr
        self._port = port or 5025
        self._scope.ip_address = addr
        self._scope.connect()
        self._connected = True
        try:
            self._instrument_id = self._scope._query("*IDN?")
        except Exception:
            self._instrument_id = "UNKNOWN"

    def close(self) -> None:
        if self._connected:
            self._scope.disconnect()
            self._connected = False

    def write(self, cmd: str) -> None:
        self._scope._write(cmd)

    def query(self, cmd: str) -> str:
        return self._scope._query(cmd)

    def query_binary(self, cmd: str) -> list[float]:
        import numpy as np
        data = self._scope.instrument.query_binary_values(
            cmd, datatype="f", is_big_endian=False, container=np.array
        )
        return data.tolist()

    def cmd(self, name: str, subs: list[tuple[str, str]]) -> str:
        tpl = self._scope.commands.get(name)
        if tpl is None:
            raise ScopeConfigurationError(f"command '{name}' not found for '{self._brand}'")
        s = tpl
        for k, v in subs:
            s = s.replace(f"{{{k}}}", v)
        return s

    def commands(self) -> list[str]:
        return sorted(self._scope.commands.keys())

    def setting(self, name: str, kwargs: list[tuple[str, str]]) -> None:
        from valicore.driver._oscilloscope_driver import _SETTINGS
        if name not in _SETTINGS:
            raise ScopeConfigurationError(f"unknown setting '{name}'")
        cmd_key, _ = _SETTINGS[name]
        cmd = self.cmd(cmd_key, kwargs)
        self.write(cmd)

    def getting(self, name: str, kwargs: list[tuple[str, str]]) -> str:
        from valicore.driver._oscilloscope_driver import _GETTERS
        if name not in _GETTERS:
            raise ScopeConfigurationError(f"unknown getting '{name}'")
        cmd_key, _ = _GETTERS[name]
        cmd = self.cmd(cmd_key, kwargs)
        return self.query(cmd)

    # ── Channel management (side-effects) ──

    def set_ch_on(self, channel: str) -> None:
        self._scope.set_channel_on(int(channel))

    def set_ch_off(self, channel: str) -> None:
        self._scope.set_channel_off(int(channel))

    def check_ch(self, channel: str) -> bool:
        cmd = self._scope.commands.get("check_ch", "CHANnel{ch}:STATe?").format(ch=channel)
        resp = self.query(cmd)
        return resp in ("1", "ON", "on")

    # ── Actions ──

    def reset(self) -> None:
        self._scope.reset()

    def autoset(self) -> None:
        cmd = self._scope.commands.get("autoset", "AUToset")
        self.write(cmd)

    def run(self) -> None:
        cmd = self._scope.commands.get("run", "RUN")
        self.write(cmd)

    def stop(self) -> None:
        cmd = self._scope.commands.get("stop", "STOP")
        self.write(cmd)

    def single(self) -> None:
        cmd = self._scope.commands.get("single", "SINGle")
        self.write(cmd)

    # ── Waveform ──

    def get_waveform(self, channel: str) -> list[float]:
        if "set_source" in self._scope.commands:
            cmd = self.cmd("set_source", [("ch", channel)])
            self.write(cmd)
        raw_cmd = self.cmd("get_raw", [("ch", channel)])
        return self.query_binary(raw_cmd)

    def get_all_waveforms(self) -> tuple[list[float], list[list[float]], dict[str, str]]:
        chs = self.active_channels()
        if not chs:
            raise ScopeConnectionError("no active channels")

        sr_resp = self.query(":ACQuire:SRATe?")
        sample_rate = float(sr_resp)

        data_matrix: list[list[float]] = []
        for ch in chs:
            try:
                wf = self.get_waveform(str(ch))
                if wf:
                    data_matrix.append(wf)
            except Exception:
                continue

        if not data_matrix:
            raise ScopeConnectionError("all channels failed")

        min_len = min(len(d) for d in data_matrix)
        data_matrix = [d[:min_len] for d in data_matrix]
        time_axis = [i / sample_rate for i in range(min_len)]

        import time
        metadata = {
            "sample_rate": f"{sample_rate:.0}",
            "channels": ",".join(str(c) for c in chs),
            "num_samples": str(min_len),
            "timestamp": str(int(time.time())),
            "instrument": self._instrument_id,
        }
        return time_axis, data_matrix, metadata


