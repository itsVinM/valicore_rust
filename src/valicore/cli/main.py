from __future__ import annotations

import json
import sys
from pathlib import Path

import click

from valicore import __version__
from valicore.campaign.loader import CampaignLoader
from valicore.campaign.runner import CampaignRunner as PythonRunner
from valicore.reporting.html_reporter import HTMLReporter
from valicore.reporting.pdf_reporter import PDFReporter
from valicore._rust import py_campaign_info, py_run_campaign


@click.group()
@click.version_option(version=__version__, prog_name="valicore")
def cli() -> None:
    """valicore — Validation Orchestration Framework"""


@cli.command()
@click.argument("campaign", type=click.Path(exists=True))
@click.option("--output", "-o", default="report", help="Output directory or file (without ext)")
@click.option("--format", "-f", "formats", multiple=True, default=["html"],
              type=click.Choice(["html", "pdf"]), help="Report format(s)")
@click.option("--instrument", "-i", "overrides", multiple=True,
              metavar="NAME=RESOURCE", help="Override instrument VISA resource")
@click.option("--scpi", default="rust", type=click.Choice(["rust", "python"]),
              help="SCPI backend: rust (tokio, fast) or python (pyvisa, compatible)")
@click.option("--json", "json_output", is_flag=True, help="Emit JSON results to stdout")
def run(
    campaign: str,
    output: str,
    formats: tuple[str, ...],
    overrides: tuple[str, ...],
    scpi: str,
    json_output: bool,
) -> None:
    """Execute a test campaign from a YAML file."""
    instrument_overrides: dict[str, str] = {}
    for ov in overrides:
        if "=" not in ov:
            click.echo(f"Invalid override format: {ov} (expected NAME=RESOURCE)", err=True)
            sys.exit(1)
        name, resource = ov.split("=", 1)
        instrument_overrides[name.strip()] = resource.strip()

    if scpi == "rust":
        results_raw = py_run_campaign(campaign)
        results = json.loads(results_raw)
    else:
        loader = CampaignLoader()
        campaign_obj = loader.load(campaign)
        runner = PythonRunner(campaign_obj, instrument_overrides=instrument_overrides)
        try:
            results = runner.run()
        finally:
            runner.close()

    if json_output:
        click.echo(json.dumps(results, indent=2, default=str))

    output_path = Path(output)
    for fmt in formats:
        if fmt == "html":
            reporter = HTMLReporter()
            dest = output_path.with_suffix(".html") if output_path.suffix else output_path / "report.html"
            dest.parent.mkdir(parents=True, exist_ok=True)
            reporter.write(results, dest, version=__version__)
            click.echo(f"HTML report: {dest.resolve()}")
        elif fmt == "pdf":
            reporter = PDFReporter()
            dest = output_path.with_suffix(".pdf") if output_path.suffix else output_path / "report.pdf"
            dest.parent.mkdir(parents=True, exist_ok=True)
            reporter.write(results, dest, version=__version__)
            click.echo(f"PDF report: {dest.resolve()}")

    total = 0
    failed = 0
    for g in results["groups"].values():
        steps = g["steps"]
        total += len(steps)
        for s in steps:
            if s["status"] == "failed":
                failed += 1

    if failed:
        click.echo(f"\n{failed}/{total} steps FAILED", err=True)
        sys.exit(1)
    click.echo(f"\nAll {total} steps passed.")


@cli.command()
@click.argument("campaign", type=click.Path(exists=True))
@click.option("--engine", type=click.Choice(["rust", "python"]), default="rust",
              help="Validation engine backend")
def validate(campaign: str, engine: str) -> None:
    """Validate a campaign YAML file without executing."""
    if engine == "rust":
        try:
            info = json.loads(py_campaign_info(campaign))
        except ValueError as e:
            click.echo(f"Validation FAILED: {e}", err=True)
            sys.exit(1)
        click.echo(f"Valid campaign: {info['title']} (v{info.get('version', '1.0')})")
        click.echo(f"  Instruments: {len(info['instruments'])}")
        click.echo(f"  Groups: {len(info['groups'])}")
        click.echo(f"  Total steps: {info['total_steps']}")
    else:
        loader = CampaignLoader()
        try:
            campaign_obj = loader.load(campaign)
        except (FileNotFoundError, ValueError) as e:
            click.echo(f"Validation FAILED: {e}", err=True)
            sys.exit(1)
        click.echo(f"Valid campaign: {campaign_obj.title} (v{campaign_obj.version})")
        click.echo(f"  Instruments: {len(campaign_obj.instruments)}")
        click.echo(f"  Groups: {len(campaign_obj.groups)}")
        total_steps = sum(len(g.steps) for g in campaign_obj.groups.values())
        click.echo(f"  Total steps: {total_steps}")


@cli.command()
@click.argument("directory", type=click.Path(exists=True))
def list_campaigns(directory: str) -> None:
    """List all campaigns found in a directory."""
    loader = CampaignLoader()
    try:
        campaigns = loader.load_all(directory)
    except NotADirectoryError as e:
        click.echo(str(e), err=True)
        sys.exit(1)

    if not campaigns:
        click.echo("No campaign files found.")
        return

    for c in campaigns:
        steps = sum(len(g.steps) for g in c.groups.values())
        click.echo(f"  {c.title:40s} v{c.version:5s}  {steps:3d} steps")


@cli.command()
@click.option("--output", "-o", default="campaign.yaml", help="Output file path")
def init(output: str) -> None:
    """Create a template campaign YAML file."""
    template = """\
title: "My Validation Campaign"
version: "1.0"
description: "TODO: describe the campaign"

instruments:
  dmm:
    kind: keysight_34460a
    resource: "TCPIP0::192.168.1.100::inst0::INSTR"
    timeout: 5000

  psu:
    kind: rs_nga100
    resource: "TCPIP0::192.168.1.101::inst0::INSTR"
    timeout: 5000

groups:
  power_up:
    description: "Power-up sequence validation"
    steps:
      - name: "Measure 3.3V rail"
        instrument: dmm
        measurements:
          - name: "Vout_3v3"
            type: "volt:dc"
            limits:
              - op: within
                value: 3.3
                tolerance: 0.05
      - name: "Measure 5.0V rail"
        instrument: dmm
        measurements:
          - name: "Vout_5v0"
            type: "volt:dc"
            limits:
              - op: within
                value: 5.0
                tolerance: 0.1

output:
  formats:
    - html
    - pdf
"""
    path = Path(output)
    if path.exists():
        click.confirm(f"{path} already exists. Overwrite?", abort=True)
    path.write_text(template)
    click.echo(f"Created template: {path.resolve()}")
