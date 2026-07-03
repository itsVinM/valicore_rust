from __future__ import annotations

import pytest

from valicore import Oscilloscope


class TestOscilloscope:
    def test_brands(self):
        brands = Oscilloscope.brands()
        assert isinstance(brands, list)
        assert len(brands) == 10
        assert "RIGOL" in brands
        assert "TEKTRONIX" in brands

    def test_new_rigol(self):
        scope = Oscilloscope("RIGOL")
        assert scope.brand() == "RIGOL"
        assert scope.is_connected() is False
        assert scope.instrument_id() == "OFFLINE"

    def test_new_rs(self):
        scope = Oscilloscope("RS")
        assert scope.brand() == "RS"

    def test_new_unknown_brand(self):
        with pytest.raises(RuntimeError, match="unknown brand"):
            Oscilloscope("NONEXISTENT")

    def test_available_settings(self):
        settings = Oscilloscope.available_settings()
        assert "vertical_scale" in settings
        assert "timebase" in settings
        assert "trigger_source" in settings

    def test_available_gettings(self):
        gettings = Oscilloscope.available_gettings()
        assert "vertical_scale" in gettings
        assert "timebase" in gettings

    def test_commands(self):
        scope = Oscilloscope("RIGOL")
        cmds = scope.commands()
        assert "reset" in cmds
        assert "autoset" in cmds
        assert "get_raw" in cmds
        assert "set_v_scale" in cmds

    def test_cmd_substitution(self):
        scope = Oscilloscope("RIGOL")
        cmd = scope.cmd("set_v_scale", [("ch", "CH1"), ("val", "1.0")])
        assert cmd == ":CH1:SCAL 1.0"

    def test_cmd_no_substitution(self):
        scope = Oscilloscope("RIGOL")
        cmd = scope.cmd("reset", [])
        assert cmd == "*RST"

    def test_cmd_with_number_channel(self):
        scope = Oscilloscope("RS")
        cmd = scope.cmd("set_v_scale", [("ch", "1"), ("val", "0.5")])
        assert cmd == "CHANnel1:SCALe 0.5"

    def test_cmd_unknown(self):
        scope = Oscilloscope("RIGOL")
        with pytest.raises(RuntimeError):
            scope.cmd("nonexistent", [])

    def test_setting_via_dispatch(self):
        """setting() generates correct SCPI via SETTINGS map."""
        scope = Oscilloscope("RIGOL")
        cmd = scope.cmd("set_v_scale", [("ch", "CH1"), ("val", "0.5")])
        assert cmd == ":CH1:SCAL 0.5"

    def test_setting_trigger_source_rs(self):
        """RS trigger source command with numeric channel."""
        scope = Oscilloscope("RS")
        cmd = scope.cmd("set_trig_source", [("ch", "1")])
        assert cmd == "TRIGger:SOURce CHANnel1"

    def test_getting_via_dispatch(self):
        """getting() generates correct SCPI via GETTINGS map."""
        scope = Oscilloscope("RIGOL")
        cmd = scope.cmd("get_v_scale", [("ch", "CH1")])
        assert cmd == ":CH1:SCAL?"
