import tempfile
from pathlib import Path

import pytest
import yaml

from valicore.campaign.loader import CampaignLoader
from valicore.campaign.model import TestCampaign
from valicore.campaign.runner import CampaignRunner

SAMPLE_CAMPAIGN = {
    "title": "Unit Test Campaign",
    "version": "1.0",
    "instruments": {
        "dmm": {
            "kind": "keysight_34460a",
            "resource": "TCPIP0::localhost::inst0::INSTR",
            "timeout": 1000,
        }
    },
    "groups": {
        "group1": {
            "name": "Group 1",
            "steps": [
                {
                    "name": "Step 1",
                    "instrument": "dmm",
                    "measurements": [
                        {
                            "name": "Vout",
                            "type": "volt:dc",
                            "limits": [{"op": "within", "value": 3.3, "tolerance": 0.1}],
                        }
                    ],
                }
            ],
        }
    },
}


@pytest.fixture
def campaign_file():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
        yaml.dump(SAMPLE_CAMPAIGN, f)
        path = f.name
    yield Path(path)
    Path(path).unlink(missing_ok=True)


class TestCampaignLoader:
    def test_load_valid(self, campaign_file):
        campaign = CampaignLoader.load(campaign_file)
        assert isinstance(campaign, TestCampaign)
        assert campaign.title == "Unit Test Campaign"

    def test_load_missing_file(self):
        with pytest.raises(FileNotFoundError):
            CampaignLoader.load("/nonexistent/campaign.yaml")

    def test_load_invalid_extension(self, tmp_path):
        p = tmp_path / "campaign.json"
        p.write_text("{}")
        with pytest.raises(ValueError, match="unsupported file type"):
            CampaignLoader.load(p)

    def test_load_all(self, tmp_path):
        for i in range(3):
            (tmp_path / f"camp{i}.yaml").write_text(
                yaml.dump({**SAMPLE_CAMPAIGN, "title": f"Campaign {i}"})
            )
        campaigns = CampaignLoader.load_all(tmp_path)
        assert len(campaigns) == 3

    def test_load_empty_not_mapping(self, tmp_path):
        p = tmp_path / "bad.yaml"
        p.write_text("12345")
        with pytest.raises(ValueError, match="top-level mapping"):
            CampaignLoader.load(p)


class TestCampaignRunner:
    def test_run_simple_campaign(self, campaign_file):
        campaign = CampaignLoader.load(campaign_file)
        runner = CampaignRunner(campaign)

        # No real instrument, we expect a connection error
        results = runner.run()
        runner.close()

        assert results["title"] == "Unit Test Campaign"
        assert "group1" in results["groups"]
        # Should fail since instrument can't connect
        step_results = results["groups"]["group1"]["steps"]
        assert len(step_results) == 1
        assert step_results[0]["status"] == "failed"
        assert "error" in step_results[0]

    @pytest.mark.parametrize(
        "op,value,tol,result",
        [
            ("eq", 5.0, None, True),
            ("eq", 5.1, None, False),
            ("eq", 5.0, 0.1, True),
            ("ne", 5.0, None, False),
            ("ne", 6.0, None, True),
            ("lt", 10.0, None, True),
            ("lt", 3.0, None, False),
            ("gt", 1.0, None, True),
            ("gt", 7.0, None, False),
            ("within", 5.0, 0.5, True),
            ("within", 6.0, 0.5, False),
        ],
    )
    def test_limit_evaluation(self, op, value, tol, result, campaign_file):
        campaign = CampaignLoader.load(campaign_file)
        runner = CampaignRunner(campaign)
        limit = type("Limit", (), {"op": op, "value": value, "tolerance": tol})()
        assert runner._check_limit(5.0, limit) == result
