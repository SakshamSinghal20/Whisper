from typing import Annotated

from pydantic import BaseModel, ConfigDict, Field, StrictInt, field_validator, model_validator

HashHex = Annotated[str, Field(pattern=r"^[0-9a-f]{64}$")]
Height = Annotated[StrictInt, Field(ge=0, le=2_147_483_647)]


class ScanRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    start_height: Height
    end_height: Height
    prefixes: list[Annotated[str, Field(pattern=r"^[0-9a-fA-F]{8}$")]] = Field(
        min_length=1, max_length=10000
    )
    cursor: str | None = Field(default=None, max_length=1024)
    include_proofs: bool = False

    @model_validator(mode="after")
    def validate_range(self):
        if self.end_height < self.start_height:
            raise ValueError("end_height must be at least start_height")
        if self.include_proofs:
            raise ValueError("Merkle proofs are not supported; set include_proofs to false")
        return self

    @field_validator("prefixes")
    @classmethod
    def normalize_prefixes(cls, values):
        return sorted({value.lower() for value in values})


class Candidate(BaseModel):
    model_config = ConfigDict(strict=True)
    txid: HashHex
    vout: Annotated[StrictInt, Field(ge=0, le=0xFFFFFFFF)]
    amount: Annotated[StrictInt, Field(ge=0, le=2_100_000_000_000_000)]
    script_pubkey: Annotated[str, Field(pattern=r"^5120[0-9a-f]{64}$")]
    block_height: Height
    block_hash: HashHex
    timestamp: Annotated[StrictInt, Field(ge=0)]


class IndexedTransaction(BaseModel):
    model_config = ConfigDict(strict=True)
    txid: HashHex
    block_height: Height
    input_tweak: Annotated[str, Field(pattern=r"^0[23][0-9a-f]{64}$")]
    outputs: list[Candidate]


class TransactionsResponse(BaseModel):
    model_config = ConfigDict(strict=True)
    transactions: list[IndexedTransaction]
    scanned_blocks: list[Height]
    chain_anchor: HashHex
    previous_hash: HashHex
    next_cursor: str | None
