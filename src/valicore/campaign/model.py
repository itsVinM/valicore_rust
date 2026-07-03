from __future__ import annotations

from enum import Enum
from typing import Any, Optional

from pydantic import BaseModel, Field


class ComparisonOp(str, Enum):
    eq = "eq"
    ne = "ne"
    lt = "lt"
    le = "le"
    gt = "gt"
    ge = "ge"
    within = "within"
    outside = "outside"


class Limit(BaseModel):
    op: ComparisonOp
    value: float
    tolerance: Optional[float] = None


class MeasurementDef(BaseModel):
    name: str
    type: str
    channel: Optional[str] = None
    range: Optional[str] = None
    resolution: Optional[str] = None
    limits: list[Limit] = Field(default_factory=list)
    options: dict[str, Any] = Field(default_factory=dict)


class InstrumentConfig(BaseModel):
    name: str = ""
    kind: str
    resource: str
    timeout: int = 5000
    options: dict[str, Any] = Field(default_factory=dict)


class PostProcessing(BaseModel):
    window: Optional[str] = None
    filter_type: Optional[str] = None
    filter_cutoff: Optional[float] = None
    filter_order: Optional[int] = None


class TestStep(BaseModel):
    name: str
    description: Optional[str] = None
    instrument: str
    command: Optional[str] = None
    measurements: list[MeasurementDef] = Field(default_factory=list)
    post_processing: Optional[PostProcessing] = None
    repeat: int = 1
    delay_ms: int = 0


class TestGroup(BaseModel):
    name: str = ""
    description: Optional[str] = None
    setup: Optional[str] = None
    teardown: Optional[str] = None
    steps: list[TestStep] = Field(default_factory=list)
    depends_on: list[str] = Field(default_factory=list)


class TestCampaign(BaseModel):
    title: str
    description: Optional[str] = None
    version: str = "1.0"
    instruments: dict[str, InstrumentConfig] = Field(default_factory=dict)
    groups: dict[str, TestGroup] = Field(default_factory=dict)
    variables: dict[str, Any] = Field(default_factory=dict)
    output: dict[str, Any] = Field(default_factory=lambda: {"formats": ["html"]})
    ci_hooks: dict[str, Any] = Field(default_factory=dict)
