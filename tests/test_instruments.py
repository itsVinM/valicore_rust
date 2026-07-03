from valicore.instruments.base import SCPIInstrument
from valicore.instruments.rigol import RigolDS1000Z, RigolDG4000
from valicore.instruments.keysight import Keysight34460A, Keysight33600A
from valicore.instruments.rohde_schwarz import RohdeSchwarzNGA100, RohdeSchwarzFPC1000


def test_instrument_registry():
    from valicore.instruments import REGISTRY, create_instrument
    from valicore.instruments.base import SCPIInstrument

    assert "rigol_ds1000z" in REGISTRY
    assert "keysight_34460a" in REGISTRY
    assert "rs_nga100" in REGISTRY
    assert issubclass(SCPIInstrument, SCPIInstrument)  # just check it's importable


def test_rigol_ds1000z_interface():
    instr = RigolDS1000Z("TCPIP0::localhost::inst0::INSTR")
    assert instr.family == "rigol_ds1000z"
    assert instr._resource == "TCPIP0::localhost::inst0::INSTR"
    assert instr._timeout == 5000


def test_rigol_dg4000_methods():
    instr = RigolDG4000("TCPIP0::localhost::inst0::INSTR")
    assert instr.family == "rigol_dg4000"


def test_keysight_34460a_methods():
    instr = Keysight34460A("TCPIP0::localhost::inst0::INSTR")
    assert instr.family == "keysight_34460a"


def test_rohde_schwarz_nga100_methods():
    instr = RohdeSchwarzNGA100("TCPIP0::localhost::inst0::INSTR")
    assert instr.family == "rs_nga100"


def test_rohde_schwarz_fpc1000_methods():
    instr = RohdeSchwarzFPC1000("TCPIP0::localhost::inst0::INSTR")
    assert instr.family == "rs_fpc1000"


def test_create_instrument():
    from valicore.instruments import create_instrument

    instr = create_instrument("rigol_ds1000z", "TCPIP0::test::INSTR")
    assert isinstance(instr, RigolDS1000Z)

    instr = create_instrument("keysight_34460a", "TCPIP0::test::INSTR")
    assert isinstance(instr, Keysight34460A)


def test_create_instrument_unknown():
    from valicore.instruments import create_instrument

    try:
        create_instrument("nonexistent", "TCPIP0::test::INSTR")
        assert False, "should have raised"
    except ValueError as e:
        assert "unknown instrument" in str(e)
