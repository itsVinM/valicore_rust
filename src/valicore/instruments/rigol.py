from __future__ import annotations

from typing import Any

import numpy as np

from valicore.instruments.base import SCPIInstrument


class RigolDS1000Z(SCPIInstrument):
    family = "rigol_ds1000z"

    def _on_connect(self) -> None:
        self.write(":SYSTem:LANGuage EN")

    def configure_measurement(self, meas_type: str, **kwargs: Any) -> None:
        source = kwargs.get("source", "CHAN1")
        self.write(f":MEASure:{meas_type.upper()} {source}")

    def read_measurement(self, meas_type: str, **kwargs: Any) -> float:
        source = kwargs.get("source", "CHAN1")
        resp = self.query(f":MEASure:{meas_type.upper()}? {source}")
        return float(resp.partition(",")[0])

    def set_channel_scale(self, channel: str, scale: float) -> None:
        self.write(f":{channel}:SCAL {scale}")

    def set_channel_offset(self, channel: str, offset: float) -> None:
        self.write(f":{channel}:OFFS {offset}")

    def set_timebase(self, scale: float) -> None:
        self.write(f":TIMebase:SCAL {scale}")

    def set_trigger(self, source: str, level: float, edge: str = "POS") -> None:
        self.write(f":TRIGger:{edge} {source},{level}")

    def get_waveform(self, source: str = "CHAN1") -> list[float]:
        self.write(f":WAVeform:SOURce {source}")
        self.write(":WAVeform:FORMat ASCII")
        raw = self.query(":WAVeform:DATA?")
        return np.fromstring(raw, sep=",", dtype=np.float64).tolist()


class RigolDG4000(SCPIInstrument):
    family = "rigol_dg4000"

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

    def set_load(self, channel: str = "OUTP1", impedance: float = 50.0) -> None:
        self.write(f":OUTPut{channel[-1]}:LOAD {impedance}")
