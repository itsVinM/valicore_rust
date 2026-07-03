from valicore.instruments.base import SCPIInstrument
from valicore.instruments.rigol import RigolDS1000Z, RigolDG4000
from valicore.instruments.keysight import Keysight34460A, Keysight33600A
from valicore.instruments.rohde_schwarz import RohdeSchwarzNGA100, RohdeSchwarzFPC1000

REGISTRY: dict[str, type[SCPIInstrument]] = {
    "rigol_ds1000z": RigolDS1000Z,
    "rigol_dg4000": RigolDG4000,
    "keysight_34460a": Keysight34460A,
    "keysight_33600a": Keysight33600A,
    "rs_nga100": RohdeSchwarzNGA100,
    "rs_fpc1000": RohdeSchwarzFPC1000,
}


def create_instrument(kind: str, resource: str, timeout: int = 5000) -> SCPIInstrument:
    cls = REGISTRY.get(kind)
    if cls is None:
        raise ValueError(f"unknown instrument kind: {kind} (available: {list(REGISTRY)})")
    return cls(resource, timeout=timeout)


__all__ = [
    "SCPIInstrument",
    "RigolDS1000Z",
    "RigolDG4000",
    "Keysight34460A",
    "Keysight33600A",
    "RohdeSchwarzNGA100",
    "RohdeSchwarzFPC1000",
    "create_instrument",
    "REGISTRY",
]
