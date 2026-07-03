from __future__ import annotations

import csv
import re
import sys
from pathlib import Path

import click

from valicore import __version__
from valicore import Oscilloscope
from valicore.core import RustSignalProcessor


def _parse_address(resource: str) -> str:
    """Extract IP address from VISA resource string or use bare IP."""
    m = re.search(r"::(\d+\.\d+\.\d+\.\d+)::", resource)
    if m:
        return m.group(1)
    return resource


def _find_visa_resources() -> list[str]:
    try:
        import pyvisa
        rm = pyvisa.ResourceManager()
        resources = rm.list_resources()
        rm.close()
        return resources
    except Exception:
        return []


@click.group()
@click.version_option(version=__version__, prog_name="valicore")
def cli() -> None:
    """valicore — oscilloscope control & signal analysis."""


@cli.command()
def resources() -> None:
    """List available oscilloscope brands, drivers, and VISA resources."""
    brands = Oscilloscope.brands()
    click.echo("YAML oscilloscope brands:")
    for b in brands:
        click.echo(f"  {b}")

    from valicore.instruments import REGISTRY
    if REGISTRY:
        click.echo("\nPython instrument drivers:")
        for name in REGISTRY:
            click.echo(f"  {name}")

    visa = _find_visa_resources()
    if visa:
        click.echo("\nVISA resources:")
        for r in visa:
            click.echo(f"  {r}")


@cli.command()
@click.option("--brand", "-b", default=None, help="Oscilloscope brand (auto-detect if omitted)")
@click.option("--resource", "-r", required=True, help="IP address or VISA resource string")
@click.option("--port", "-p", type=int, default=None, help="TCP port (defaults to brand's ip_config.port)")
@click.option("--channel", "-c", default="CH1", help="Channel to capture (e.g. CH1, 1)")
@click.option("--output", "-o", default=None, help="Write samples to CSV")
@click.option("--fft", is_flag=True, help="Print FFT peak frequency")
@click.option("--stats", is_flag=True, help="Print signal stats")
def capture(brand: str | None, resource: str, port: int | None, channel: str, output: str | None, fft: bool, stats: bool) -> None:
    """Capture waveform data from an oscilloscope."""
    addr = _parse_address(resource)

    if brand is None:
        click.echo("Auto-detecting brand...")
        detected = Oscilloscope.detect_brand(addr, port)
        brand = detected
        click.echo(f"Detected: {brand}")
    else:
        brands = Oscilloscope.brands()
        if brand.upper() not in [b.upper() for b in brands]:
            click.echo(f"Unknown brand: {brand}. Available: {', '.join(brands)}", err=True)
            sys.exit(1)

    scope = Oscilloscope(brand)

    try:
        scope.connect(addr, port)
        click.echo(f"Connected: {scope.instrument_id()}")
        data = scope.get_waveform(channel)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)
    finally:
        scope.close()

    click.echo(f"Captured {len(data)} samples from {channel}")

    if output:
        path = Path(output)
        with open(path, "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["sample", "value"])
            for i, v in enumerate(data):
                w.writerow([i, v])
        click.echo(f"Saved to {path}")

    if fft:
        sr = 1e6
        result = RustSignalProcessor.fft(data, sr)
        peak_idx = result["magnitudes"].index(max(result["magnitudes"]))
        click.echo(f"FFT peak: {result['frequencies_hz'][peak_idx]:.1f} Hz")

    if stats:
        s = RustSignalProcessor.stats(data)
        click.echo(f"Stats: mean={s['mean']:.4f}, rms={s['rms']:.4f}, "
                   f"min={s['min']:.4f}, max={s['max']:.4f}")


@cli.command()
@click.argument("file", type=click.Path(exists=True))
@click.option("--fft", is_flag=True, help="Compute FFT")
@click.option("--stats", is_flag=True, help="Compute statistics")
@click.option("--thd", type=float, default=None, metavar="FREQ_HZ", help="Compute THD at fundamental frequency")
def analyze(file: str, fft: bool, stats: bool, thd: float | None) -> None:
    """Analyze signal data from a CSV file (column: value)."""
    samples: list[float] = []
    with open(file) as f:
        reader = csv.DictReader(f)
        for row in reader:
            samples.append(float(row["value"]))

    click.echo(f"Loaded {len(samples)} samples from {file}")

    if fft:
        sr = 1e6
        result = RustSignalProcessor.fft(samples, sr)
        peak_idx = result["magnitudes"].index(max(result["magnitudes"]))
        click.echo(f"FFT peak: {result['frequencies_hz'][peak_idx]:.1f} Hz")

    if stats:
        s = RustSignalProcessor.stats(samples)
        click.echo(f"Stats: mean={s['mean']:.4f}, rms={s['rms']:.4f}, "
                   f"min={s['min']:.4f}, max={s['max']:.4f}")

    if thd is not None:
        sr = 1e6
        val = RustSignalProcessor.thd(samples, thd, sr)
        click.echo(f"THD: {val:.2f}%")
