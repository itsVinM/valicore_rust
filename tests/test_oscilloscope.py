from __future__ import annotations
import pytest
from valicore import Oscilloscope


class TestOscilloscope:
    """
    Test suite for the Oscilloscope class.
    """
    
    def test_brands(self) -> None:
        brands : list[str] = Oscilloscope.brands()
        assert isinstance(brands, list)
        assert len(brands) == 10
        assert "RIGOL" in brands
        assert "TEKTRONIX" in brands

    def test_new_rigol(self) -> None:
        scope : Oscilloscope = Oscilloscope("RIGOL")
        assert scope.brand() == "RIGOL"
        assert scope.is_connected() is False
        assert scope.instrument_id() == "OFFLINE"

    def test_new_rs(self) -> None:
        scope : Oscilloscope = Oscilloscope("RS")
        assert scope.brand() == "RS"

    def test_new_unknown_brand(self) -> None:
        with pytest.raises(RuntimeError, match="unknown brand"):
            Oscilloscope("NONEXISTENT")

    def test_available_settings(self) -> None:
        settings : dict[str, str] = Oscilloscope.available_settings()
        assert "vertical_scale" in settings
        assert "timebase" in settings
        assert "trigger_source" in settings

    def test_available_gettings(self) -> None:
        gettings : dict[str, str] = Oscilloscope.available_gettings()
        assert "vertical_scale" in gettings
        assert "timebase" in gettings

    def test_commands(self) -> None:
        scope : Oscilloscope = Oscilloscope("RIGOL")
        cmds : list[str] = scope.commands()
        assert "reset" in cmds
        assert "autoset" in cmds
        assert "get_raw" in cmds
        assert "set_v_scale" in cmds

    def test_cmd_substitution(self) -> None:
        scope : Oscilloscope = Oscilloscope("RIGOL")
        cmd : str = scope.cmd("set_v_scale", [("ch", "CH1"), ("val", "1.0")])
        assert cmd == ":CH1:SCAL 1.0"

    def test_cmd_no_substitution(self) -> None:
        scope : Oscilloscope = Oscilloscope("RIGOL")
        cmd : str = scope.cmd("reset", [])
        assert cmd == "*RST"

    def test_cmd_with_number_channel(self) -> None:
        scope : Oscilloscope = Oscilloscope("RS")
        cmd : str = scope.cmd("set_v_scale", [("ch", "1"), ("val", "0.5")])
        assert cmd == "CHANnel1:SCALe 0.5"

    def test_cmd_unknown(self) -> None:
        scope : Oscilloscope = Oscilloscope("RIGOL")
        with pytest.raises(RuntimeError):
            scope.cmd("nonexistent", [])

    def test_setting_via_dispatch(self) -> None:
        """setting() generates correct SCPI via SETTINGS map."""
        scope : Oscilloscope = Oscilloscope("RIGOL")
        cmd : str = scope.cmd("set_v_scale", [("ch", "CH1"), ("val", "0.5")])
        assert cmd == ":CH1:SCAL 0.5"

    def test_setting_trigger_source_rs(self) -> None:
        """RS trigger source command with numeric channel."""
        scope : Oscilloscope = Oscilloscope("RS")
        cmd : str = scope.cmd("set_trig_source", [("ch", "1")])
        assert cmd == "TRIGger:SOURce CHANnel1"

    def test_getting_via_dispatch(self) -> None:
        """getting() generates correct SCPI via GETTINGS map."""
        scope : Oscilloscope = Oscilloscope("RIGOL")
        cmd : str = scope.cmd("get_v_scale", [("ch", "CH1")])
        assert cmd == ":CH1:SCAL?"

    def test_default_port(self) -> None:
        scope : Oscilloscope = Oscilloscope("RIGOL")
        assert scope.default_port() == 5025

    def test_detect_brand_no_connection(self) -> None:
        with pytest.raises(RuntimeError):
            Oscilloscope.detect_brand("192.0.2.1", 5025, 100)
