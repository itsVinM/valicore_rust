from __future__ import annotations

from pathlib import Path

import yaml
from pydantic import ValidationError

from valicore.campaign.model import TestCampaign


class CampaignLoader:
    @staticmethod
    def load(path: str | Path) -> TestCampaign:
        path = Path(path)
        if not path.exists():
            raise FileNotFoundError(f"campaign file not found: {path}")
        if path.suffix not in (".yaml", ".yml"):
            raise ValueError(f"unsupported file type: {path.suffix} (expected .yaml/.yml)")

        raw = path.read_text(encoding="utf-8")
        data = yaml.safe_load(raw)
        if not isinstance(data, dict):
            raise ValueError("campaign file must contain a top-level mapping")

        raw_configs = data.get("instruments", {})
        for name, cfg in raw_configs.items():
            if isinstance(cfg, dict) and not cfg.get("name"):
                cfg["name"] = name

        raw_groups = data.get("groups", {})
        for name, grp in raw_groups.items():
            if isinstance(grp, dict) and not grp.get("name"):
                grp["name"] = name

        try:
            return TestCampaign(**data)
        except ValidationError as e:
            raise ValueError(f"campaign validation failed:\n{e}") from e

    @staticmethod
    def load_all(directory: str | Path) -> list[TestCampaign]:
        directory = Path(directory)
        if not directory.is_dir():
            raise NotADirectoryError(f"not a directory: {directory}")

        campaigns: list[TestCampaign] = []
        for p in sorted(directory.glob("*.yaml")) + sorted(directory.glob("*.yml")):
            campaigns.append(CampaignLoader.load(p))
        return campaigns
