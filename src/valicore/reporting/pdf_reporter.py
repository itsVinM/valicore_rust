from __future__ import annotations

from pathlib import Path
from typing import Any

from valicore.reporting.html_reporter import HTMLReporter


class PDFReporter:
    def __init__(self) -> None:
        self._html_reporter = HTMLReporter()

    def render(self, results: dict[str, Any], version: str = "0.1.0") -> bytes:
        return self._html_to_pdf(results, version)

    def write(
        self, results: dict[str, Any], path: str | Path, version: str = "0.1.0"
    ) -> Path:
        path = Path(path)
        pdf_data = self.render(results, version=version)
        path.write_bytes(pdf_data)
        return path

    def _html_to_pdf(self, results: dict[str, Any], version: str) -> bytes:
        html = self._html_reporter.render(results, version=version)
        try:
            from weasyprint import HTML as WeasyPrintHTML

            doc = WeasyPrintHTML(string=html).render()
            return doc.write_pdf()
        except ImportError:
            msg = (
                "weasyprint is required for PDF generation. "
                "Install it with: pip install weasyprint"
            )
            raise RuntimeError(msg) from None
