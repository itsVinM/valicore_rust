from __future__ import annotations

from typing import Any

import numpy as np

from valicore.instruments.base import SCPIInstrument


class Keysight34460A(SCPIInstrument):
    family = "keysight_34460a"

    def configure_measurement(self, meas_type: str, **kwargs: Any) -> None:
        meas_type = meas_type.upper()
        if meas_type == "VOLT:DC":
            range_val = kwargs.get("range", "10")
            resolution = kwargs.get("resolution", "0.001")
            self.write(f":CONFigure:VOLTage:DC {range_val},{resolution}")
        elif meas_type == "VOLT:AC":
            self.write(f":CONFigure:VOLTage:AC {kwargs.get('range', '10')}")
        elif meas_type == "CURR:DC":
            self.write(f":CONFigure:CURRent:DC {kwargs.get('range', '1')}")
        elif meas_type == "RES":
            self.write(f":CONFigure:RESistance {kwargs.get('range', '1e6')}")
        else:
            self.write(f":CONFigure:{meas_type}")

    def read_measurement(self, meas_type: str, **kwargs: Any) -> float:
        self.configure_measurement(meas_type, **kwargs)
        resp = self.query(":READ?")
        return float(resp.partition(",")[0])

    def configure_scan(self, channels: list[str]) -> None:
        chan_str = ",".join(f"(@{c})" for c in channels)
        self.write(f":ROUTe:SCAN {chan_str}")
        self.write(":ROUTe:SCAN:COUNt 1")

    def fetch_scan(self) -> list[float]:
        raw = self.query(":FETCh?")
        return np.fromstring(raw, sep=",", dtype=np.float64).tolist()

    def set_range(self, meas_type: str, range_val: float) -> None:
        self.write(f":{meas_type}:RANGe {range_val}")

    def set_nplc(self, nplc: int = 10) -> None:
        self.write(f":VOLT:DC:NPLC {nplc}")


class Keysight33600A(SCPIInstrument):
    family = "keysight_33600a"

    def configure_measurement(self, meas_type: str, **kwargs: Any) -> None:
        pass

    def read_measurement(self, meas_type: str, **kwargs: Any) -> float:
        return 0.0

    def set_waveform(
        self,
        channel: str = "OUTP1",
        shape: str = "SIN",
        freq: float = 1000.0,
        amplitude: float = 1.0,
        offset: float = 0.0,
    ) -> None:
        self.write(f":SOURce{channel[-1]}:APPLy:{shape} {freq},{amplitude},{offset}")

    def set_output(self, channel: str = "OUTP1", state: bool = True) -> None:
        self.write(f":OUTPut{channel[-1]}:STATe {1 if state else 0}")

    def set_sweep(
        self,
        channel: str = "OUTP1",
        start_freq: float = 100.0,
        stop_freq: float = 1e6,
        duration: float = 1.0,
    ) -> None:
        c = channel[-1]
        self.write(f":SOURce{c}:SWEep:FREQ:STARt {start_freq}")
        self.write(f":SOURce{c}:SWEep:FREQ:STOP {stop_freq}")
        self.write(f":SOURce{c}:SWEep:TIME {duration}")
        self.write(f":SOURce{c}:SWEep:STATe ON")
