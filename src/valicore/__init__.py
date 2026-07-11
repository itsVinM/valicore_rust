from valicore.core import Oscilloscope, RustSignalProcessor

try:
    from valicore._rust import py_save_csv as save_csv, py_save_h5 as save_h5
except ImportError:
    save_csv = None
    save_h5 = None

__version__ = "0.1.0"

__all__ = ["Oscilloscope", "RustSignalProcessor", "save_csv", "save_h5"]
