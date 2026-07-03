import pytest
from valicore.reporting.html_reporter import HTMLReporter
from valicore.reporting.pdf_reporter import PDFReporter

SAMPLE_RESULTS = {
    "title": "Test Report",
    "version": "1.0",
    "timestamp": "2026-07-02T12:00:00",
    "groups": {
        "group1": {
            "name": "Power Tests",
            "description": "Check voltages",
            "status": "passed",
            "steps": [
                {
                    "name": "Measure 3.3V",
                    "status": "passed",
                    "error": None,
                    "measurements": [
                        {
                            "name": "Vout",
                            "value": 3.31,
                            "limits": [{"op": "within", "value": 3.3, "tolerance": 0.05}],
                            "verdict": "passed",
                        }
                    ],
                }
            ],
        }
    },
}


class TestHTMLReporter:
    def test_render(self):
        reporter = HTMLReporter()
        html = reporter.render(SAMPLE_RESULTS)
        assert "<html" in html
        assert "Test Report" in html
        assert "Power Tests" in html
        assert "3.31" in html
        assert "passed" in html

    def test_write(self, tmp_path):
        reporter = HTMLReporter()
        dest = tmp_path / "report.html"
        result = reporter.write(SAMPLE_RESULTS, dest)
        assert result.exists()
        assert result.read_text().startswith("<!DOCTYPE html>")


class TestPDFReporter:
    def test_render_fails_without_weasyprint_lib(self):
        reporter = PDFReporter()
        with pytest.raises((RuntimeError, OSError)):
            reporter.render(SAMPLE_RESULTS)
