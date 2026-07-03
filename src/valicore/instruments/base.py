from __future__ import annotations

import time
from abc import ABC, abstractmethod
from typing import Any, Optional

import pyvisa


class SCPIInstrument(ABC):
    def __init__(self, resource: str, timeout: int = 5000):
        self._resource = resource
        self._timeout = timeout
        self._rm: Optional[pyvisa.ResourceManager] = None
        self._conn: Optional[pyvisa.resources.Resource] = None

    def connect(self) -> None:
        if self._conn is not None:
            return
        self._rm = pyvisa.ResourceManager()
        self._conn = self._rm.open_resource(self._resource)
        self._conn.timeout = self._timeout
        self._conn.write_termination = "\n"
        self._conn.read_termination = "\n"
        self._on_connect()

    def _on_connect(self) -> None:
        pass

    def close(self) -> None:
        if self._conn:
            try:
                self._conn.close()
            except Exception:
                pass
            self._conn = None
        if self._rm:
            self._rm.close()
            self._rm = None

    def write(self, command: str) -> None:
        self.connect()
        self._conn.write(command)

    def query(self, command: str, delay: float = 0.0) -> str:
        self.connect()
        if delay:
            time.sleep(delay)
        return self._conn.query(command).strip()

    def query_binary(self, command: str, dtype: str = "f", delay: float = 0.0) -> bytes:
        self.connect()
        if delay:
            time.sleep(delay)
        raw = self._conn.query_binary_values(command, dtype=dtype, container=bytes)
        if isinstance(raw, bytes):
            return raw
        return b""

    def idn(self) -> str:
        return self.query("*IDN?")

    def reset(self) -> None:
        self.write("*RST")

    def clear_status(self) -> None:
        self.write("*CLS")

    def wait_complete(self) -> None:
        self.query("*OPC?")

    @abstractmethod
    def configure_measurement(self, meas_type: str, **kwargs: Any) -> None:
        ...

    @abstractmethod
    def read_measurement(self, meas_type: str, **kwargs: Any) -> float:
        ...

    def __enter__(self) -> SCPIInstrument:
        self.connect()
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()
