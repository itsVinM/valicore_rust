from __future__ import annotations

from typing import Any

import numpy as np

from valicore.instruments.base import SCPIInstrument


class RohdeSchwarzNGA100(SCPIInstrument):
    family = "rs_nga100"

    def _on_connect(self) -> None:
        self.write("SYST:REM")

    def configure_measurement(self, meas_type: str, **kwargs: Any) -> None:
        pass

    def read_measurement(self, meas_type: str, **kwargs: Any) -> float:
        if meas_type.upper() == "VOLT":
            resp = self.query("MEAS:VOLT?")
        elif meas_type.upper() == "CURR":
            resp = self.query("MEAS:CURR?")
        elif meas_type.upper() == "POW":
            resp = self.query("MEAS:POW?")
        else:
            resp = self.query(f"MEAS:{meas_type}?")
        return float(resp.strip())

    def set_voltage(self, channel: int = 1, voltage: float = 0.0) -> None:
        self.write(f"INST:NSEL {channel}")
        self.write(f"VOLT {voltage}")

    def set_current(self, channel: int = 1, current: float = 0.0) -> None:
        self.write(f"INST:NSEL {channel}")
        self.write(f"CURR {current}")

    def set_output(self, channel: int = 1, state: bool = True) -> None:
        self.write(f"INST:NSEL {channel}")
        self.write(f"OUTP:STAT {1 if state else 0}")

    def set_ovp(self, channel: int = 1, voltage: float = 30.0) -> None:
        self.write(f"INST:NSEL {channel}")
        self.write(f"VOLT:PROT {voltage}")


class RohdeSchwarzFPC1000(SCPIInstrument):
    family = "rs_fpc1000"

    def _on_connect(self) -> None:
        self.write("SYST:PRES")

    def configure_measurement(self, meas_type: str, **kwargs: Any) -> None:
        pass

    def read_measurement(self, meas_type: str, **kwargs: Any) -> float:
        freq = kwargs.get("frequency", 1e6)
        self.write(f"SENS:FREQ:CENT {freq}")
        resp = self.query("CALC:MARK1:Y?")
        return float(resp.strip())

    def set_frequency_span(self, center: float, span: float) -> None:
        self.write(f"SENS:FREQ:CENT {center}")
        self.write(f"SENS:FREQ:SPAN {span}")

    def set_resolution_bandwidth(self, rbw: float) -> None:
        self.write(f"SENS:BAND {rbw}")

    def set_amplitude_range(self, ref_level: float) -> None:
        self.write(f"DISP:WIND:TRAC:Y:RLEV {ref_level}")

    def set_marker(self, marker: int = 1, frequency: float | None = None) -> tuple[float, float]:
        if frequency is not None:
            self.write(f"CALC:MARK{marker}:X {frequency}")
        freq = float(self.query(f"CALC:MARK{marker}:X?"))
        amp = float(self.query(f"CALC:MARK{marker}:Y?"))
        return (freq, amp)

    def init_measurement(self) -> None:
        self.write("INIT:CONT OFF")
        self.write("INIT")

    def fetch_trace(self, trace: int = 1) -> list[float]:
        raw = self.query(f"TRAC:DATA? TRACE{trace}")
        return np.fromstring(raw, sep=",", dtype=np.float64).tolist()
