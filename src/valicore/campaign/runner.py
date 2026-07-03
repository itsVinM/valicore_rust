from __future__ import annotations

import time
from datetime import datetime, timezone
from typing import Any, Callable

from valicore.campaign.model import TestCampaign, ComparisonOp
from valicore.core import RustSignalProcessor
from valicore.instruments.base import SCPIInstrument


def _eq(value: float, target: float, tol: float | None) -> bool:
    return abs(value - target) <= (tol or 1e-9)


def _ne(value: float, target: float, tol: float | None) -> bool:
    return abs(value - target) > (tol or 1e-9)


def _lt(value: float, target: float, _tol: float | None) -> bool:
    return value < target


def _le(value: float, target: float, _tol: float | None) -> bool:
    return value <= target


def _gt(value: float, target: float, _tol: float | None) -> bool:
    return value > target


def _ge(value: float, target: float, _tol: float | None) -> bool:
    return value >= target


def _within(value: float, target: float, tol: float | None) -> bool:
    return abs(value - target) <= (tol or 0.0)


def _outside(value: float, target: float, tol: float | None) -> bool:
    return abs(value - target) > (tol or 0.0)


_CHECK_LIMIT_FUNCS: dict[ComparisonOp, Callable[[float, float, float | None], bool]] = {
    ComparisonOp.eq: _eq,
    ComparisonOp.ne: _ne,
    ComparisonOp.lt: _lt,
    ComparisonOp.le: _le,
    ComparisonOp.gt: _gt,
    ComparisonOp.ge: _ge,
    ComparisonOp.within: _within,
    ComparisonOp.outside: _outside,
}


class CampaignRunner:
    def __init__(self, campaign: TestCampaign, instrument_overrides: dict[str, str] | None = None):
        self.campaign = campaign
        self.results: dict[str, Any] = {
            "title": campaign.title,
            "version": campaign.version,
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "groups": {},
        }
        self._instruments: dict[str, SCPIInstrument] = {}
        self._instrument_overrides = instrument_overrides or {}
        self._signal = RustSignalProcessor()

    def run(self) -> dict[str, Any]:
        groups = self.results["groups"]
        for group_name, group in self.campaign.groups.items():
            groups[group_name] = self._run_group(group_name, group)
        return self.results

    def _run_group(self, group_name: str, group: Any) -> dict[str, Any]:
        group_result: dict[str, Any] = {
            "name": group.name,
            "description": group.description,
            "status": "skipped",
            "steps": [],
        }

        if group.setup:
            self._execute_setup(group.setup)

        steps = []
        for step in group.steps:
            step_result = self._run_step(step)
            steps.append(step_result)
            if step_result["status"] == "failed":
                break

        group_result["steps"] = steps
        all_passed = True
        for s in steps:
            if s["status"] != "passed":
                all_passed = False
                break
        group_result["status"] = "passed" if all_passed else "failed"

        if group.teardown:
            self._execute_teardown(group.teardown)

        return group_result

    def _run_step(self, step: Any) -> dict[str, Any]:
        step_result: dict[str, Any] = {
            "name": step.name,
            "description": step.description,
            "status": "passed",
            "measurements": [],
            "error": None,
        }

        try:
            instr = self._get_instrument(step.instrument)

            if step.repeat > 0:
                delay_s = step.delay_ms / 1000.0
                for i in range(step.repeat):
                    if i > 0 and delay_s:
                        time.sleep(delay_s)

                    raw_data = None
                    if step.command:
                        instr.write(step.command)
                        raw_data = instr.query(step.command)

                    measure_funcs = _CHECK_LIMIT_FUNCS
                    for meas in step.measurements:
                        value = self._acquire_measurement(instr, meas, raw_data)
                        if step.post_processing:
                            value = self._post_process(value, step.post_processing)
                        verdict = "passed"
                        if meas.limits:
                            for l in meas.limits:
                                check = measure_funcs.get(ComparisonOp(l.op))
                                if check is not None and not check(value, l.value, l.tolerance):
                                    verdict = "failed"
                                    break
                        step_result["measurements"].append(
                            {
                                "name": meas.name,
                                "value": value,
                                "limits": [l.model_dump() for l in meas.limits],
                                "verdict": verdict,
                            }
                        )
                        if verdict != "passed":
                            step_result["status"] = "failed"

        except Exception as e:
            step_result["status"] = "failed"
            step_result["error"] = str(e)

        return step_result

    def _get_instrument(self, name: str) -> SCPIInstrument:
        instruments = self._instruments
        cached = instruments.get(name)
        if cached is not None:
            return cached

        config = self.campaign.instruments.get(name)
        if not config:
            raise ValueError(f"instrument '{name}' not defined in campaign")

        resource = self._instrument_overrides.get(name, config.resource)
        instr = SCPIInstrument(resource, timeout=config.timeout)
        instruments[name] = instr
        return instr

    def _acquire_measurement(self, instr: SCPIInstrument, meas: Any, raw_data: Any) -> float:
        if meas.command:
            response = instr.query(meas.command)
            return float(response.strip())
        if raw_data is not None:
            parts = raw_data.strip().split(",")
            return float(parts[0])
        return 0.0

    def _post_process(self, value: Any, pp: Any) -> Any:
        if isinstance(value, list):
            samples = value
        else:
            samples = [float(value)]

        if pp.window:
            samples = self._signal.window(samples, pp.window)
        if pp.filter_type:
            samples = self._signal.filter(
                samples,
                filter_type=pp.filter_type,
                cutoff=pp.filter_cutoff or 0.5,
                order=pp.filter_order or 2,
            )
        if samples:
            return samples[0]
        return value

    def _check_limit(self, value: float, limit: Any) -> bool:
        check = _CHECK_LIMIT_FUNCS.get(ComparisonOp(limit.op))
        if check is None:
            return False
        return check(value, limit.value, limit.tolerance)

    def close(self) -> None:
        for instr in self._instruments.values():
            try:
                instr.close()
            except Exception:
                pass
