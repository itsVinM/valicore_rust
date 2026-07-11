from __future__ import annotations

from pathlib import Path
from typing import Any, Optional

import numpy as np
import pyvisa
import yaml


DATA_CH: int = 1
CLOCK_CH: int = 3
DATA_CH_IDX: int = 0
CLOCK_CH_IDX: int = 1

# Maps Python method name → (YAML command key, parameter names)
_SETTINGS: dict[str, tuple[str, list[str]]] = {
    "set_v_scale": ("set_v_scale", ["ch", "val"]),
    "set_v_offset": ("set_v_offset", ["ch", "val"]),
    "set_coupling": ("set_coupling", ["ch", "val"]),
    "set_h_scale": ("set_h_scale", ["val"]),
    "set_h_pos": ("set_h_pos", ["val"]),
    "set_trig_source": ("set_trig_source", ["ch"]),
    "set_trig_level": ("set_trig_level", ["val"]),
    "set_trig_slope": ("set_trig_slope", ["val"]),
    "set_ch_on": ("set_ch_on", ["ch"]),
    "set_ch_off": ("set_ch_off", ["ch"]),
}

_GETTERS: dict[str, tuple[str, list[str]]] = {
    "get_v_scale": ("get_v_scale", ["ch"]),
    "get_v_offset": ("get_v_offset", ["ch"]),
    "get_coupling": ("get_coupling", ["ch"]),
    "get_h_scale": ("get_h_scale", []),
    "get_h_pos": ("get_h_pos", []),
    "get_trig_source": ("get_trig_source", []),
    "get_trig_level": ("get_trig_level", []),
    "get_trig_slope": ("get_trig_slope", []),
}


class ScopeConnectionError(Exception):
    pass


class ScopeConfigurationError(Exception):
    pass


class ScopeAutomation:

    def __init__(self, path: Path = Path("utils/scope_config.yaml"), brand: str = "RS") -> None:
        self.rm: pyvisa.ResourceManager = pyvisa.ResourceManager()
        self.instrument: Optional[pyvisa.resources.MessageBasedResource] = None
        self.brand: str = brand
        self.commands: dict[str, str] = {}
        self.idx: str = "OFFLINE"
        self.ip_address: Optional[str] = None

        self.data_ch: int = DATA_CH
        self.clock_ch: int = CLOCK_CH
        self.data_ch_idx: int = DATA_CH_IDX
        self.clock_ch_idx: int = CLOCK_CH_IDX

        with open(path, "r") as f:
            self.lib: dict[str, Any] = yaml.safe_load(f)["OSCILLOSCOPES"]

        if self.brand not in self.lib:
            raise ScopeConnectionError(
                f"Scope brand '{self.brand}' not found in configuration file."
            )

    def _write(self, cmd: str) -> None:
        assert self.instrument is not None, "No instrument connected."
        self.instrument.write(cmd)

    def _query(self, cmd: str) -> str:
        assert self.instrument is not None, "No instrument connected."
        return self.instrument.query(cmd).strip()

    def connect(self) -> bool:
        """Connect to the oscilloscope and initialize settings."""
        self.ip_address = self.lib[self.brand].get("default_ip")
        if not self.ip_address:
            raise ScopeConnectionError(
                f"IP address for scope brand '{self.brand}' not found in configuration file."
            )

        resource_string: str = f"TCPIP0::{self.ip_address}::INSTR"

        try:
            self.instrument = self.rm.open_resource(
                resource_string,
                timeout=5000,
                access_mode=pyvisa.constants.AccessModes.shared,
            )
            assert isinstance(
                self.instrument, pyvisa.resources.MessageBasedResource
            ), "Connected resource is not a MessageBasedResource."
            self.instrument.chunk_size = 1024 * 1024
            self.instrument.write_termination = "\n"
            self.instrument.read_termination = "\n"

            self.commands = self.lib[self.brand].get("cmds", {})
            if not self.commands:
                raise ScopeConnectionError(
                    f"Commands for scope brand '{self.brand}' not found in configuration file."
                )

            quirks_cmd: str = self.lib[self.brand].get("quirks", "*CLS")
            try:
                self._write(quirks_cmd)
            except pyvisa.errors.VisaIOError as e:
                raise ScopeConfigurationError(
                    f"VISA IO error while applying initialization quirks at {resource_string}: {e}"
                ) from e

            self.idx = self.brand
            return True

        except pyvisa.errors.VisaIOError as e:
            raise ScopeConnectionError(
                f"VISA IO error while connecting to oscilloscope at {resource_string}: {e}"
            ) from e
        except KeyError as e:
            raise ScopeConnectionError(
                f"YAML missing required key for scope brand '{self.brand}': {e}"
            ) from e

    def disconnect(self) -> None:
        assert self.instrument is not None, "No instrument connected."
        try:
            self.instrument.close()
            self.idx = "OFFLINE"
        except pyvisa.errors.VisaIOError as e:
            raise ScopeConnectionError(
                f"VISA IO error while disconnecting from oscilloscope at {self.ip_address}: {e}"
            ) from e

    def reset(self) -> None:
        assert self.instrument is not None, "No instrument connected."
        self._write(self.commands.get("reset", "*RST"))

    def set_channel_on(self, ch: int) -> None:
        assert self.instrument is not None, "No instrument connected."
        cmd = self.commands.get("set_ch_on", "CHANnel{ch}:STATe ON").format(ch=ch)
        self._write(cmd)

    def set_channel_off(self, ch: int) -> None:
        assert self.instrument is not None, "No instrument connected."
        cmd = self.commands.get("set_ch_off", "CHANnel{ch}:STATe OFF").format(ch=ch)
        self._write(cmd)

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)

        if name in _SETTINGS:
            cmd_key, params = _SETTINGS[name]

            def setter(*args: Any) -> None:
                if len(args) != len(params):
                    raise TypeError(
                        f"{name}() takes {len(params)} argument(s) but {len(args)} were given."
                    )
                fmt_dict = dict(zip(params, args))
                self._write(self.commands[cmd_key].format(**fmt_dict))

            return setter

        if name in _GETTERS:
            cmd_key, params = _GETTERS[name]

            def getter(*args: Any) -> str:
                if len(args) != len(params):
                    raise TypeError(
                        f"{name}() takes {len(params)} argument(s) but {len(args)} were given."
                    )
                fmt_dict = dict(zip(params, args))
                return self._query(self.commands[cmd_key].format(**fmt_dict))

            return getter

        raise AttributeError(f"'ScopeAutomation' object has no attribute '{name}'")

    def get_waveform(
        self,
    ) -> tuple[Optional[np.ndarray], Optional[np.ndarray], Optional[dict[str, Any]]]:
        assert self.instrument is not None, "No instrument connected."

        channels = [self.data_ch]
        if self.clock_ch is not None:
            channels.append(self.clock_ch)

        data_list: list[np.ndarray] = []
        for ch in channels:
            get_raw_cmd = self.commands.get("get_raw", "CHANnel{ch}:DATA:VALues?")
            cmd = get_raw_cmd.format(ch=ch)
            try:
                data: np.ndarray = self.instrument.query_binary_values(
                    cmd, datatype="f", is_big_endian=False, container=np.array
                )
                if data.size != 0:
                    data_list.append(data)
            except Exception:
                continue

        if not data_list:
            return None, None, None

        min_len = min(d.shape[0] for d in data_list)
        data_list = [d[:min_len] for d in data_list]
        datamatrix = np.vstack(data_list)

        sample_rate = float(self.commands.get("get_h_scale", "1e-9"))
        timeaxis = np.arange(datamatrix.shape[1]) * sample_rate

        metadata: dict[str, Any] = {
            "sample_rate": sample_rate,
            "instrument": str(self.instrument),
            "brand": self.brand,
            "data_channel": self.data_ch,
            "clock_channel": self.clock_ch,
        }
        return timeaxis, datamatrix, metadata
