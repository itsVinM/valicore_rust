from __future__ import annotations

import csv
import sys
from pathlib import Path

import click

from valicore import __version__
from valicore.core import RustSignalProcessor


def _find_instruments() -> list[str]:
    """List available VISA resources."""
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
    """List available VISA instruments."""
    from valicore.instruments import REGISTRY
    click.echo("Drivers:")
    for name in REGISTRY:
        click.echo(f"  {name}")
    visa = _find_instruments()
    if visa:
        click.echo("\nVISA resources:")
        for r in visa:
            click.echo(f"  {r}")


@cli.command()
@click.option("--resource", "-r", required=True, help="VISA resource string")
@click.option("--kind", "-k", default="rigol_ds1000z", help="Instrument driver")
@click.option("--channel", "-c", default="CH1", help="Channel to capture")
@click.option("--samples", "-n", default=10000, help="Number of samples")
@click.option("--output", "-o", default=None, help="Write samples to CSV")
@click.option("--fft", is_flag=True, help="Print FFT peak frequency")
@click.option("--stats", is_flag=True, help="Print signal stats")
def capture(resource: str, kind: str, channel: str, samples: int, output: str | None, fft: bool, stats: bool) -> None:
    """Capture waveform data from an oscilloscope."""
    from valicore.instruments import create_instrument, REGISTRY

    if kind not in REGISTRY:
        click.echo(f"Unknown instrument: {kind}. Available: {', '.join(REGISTRY)}", err=True)
        sys.exit(1)
    instr = create_instrument(kind, resource)
    try:
        instr.connect()
        data = instr.get_waveform(channel)
    finally:
        instr.close()

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
    import csv
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



